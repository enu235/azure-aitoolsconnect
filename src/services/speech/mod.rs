use async_trait::async_trait;
use serde::Deserialize;

use crate::config::Cloud;
use crate::services::{measure_time, AzureService, InputType, TestContext, TestResult, TestScenario};

/// Speech Service implementation
pub struct SpeechService;

impl SpeechService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SpeechService {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Voice {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    display_name: String,
    #[allow(dead_code)]
    local_name: String,
    #[allow(dead_code)]
    short_name: String,
    #[allow(dead_code)]
    locale: String,
}

#[async_trait]
impl AzureService for SpeechService {
    fn name(&self) -> &'static str {
        "speech"
    }

    fn display_name(&self) -> &'static str {
        "Speech"
    }

    fn get_endpoint(&self, region: &str, cloud: Cloud, custom_endpoint: Option<&str>) -> String {
        if let Some(endpoint) = custom_endpoint {
            return endpoint.to_string();
        }
        match cloud {
            Cloud::Global => format!("https://{}.api.cognitive.microsoft.com", region),
            Cloud::China => format!("https://{}.api.cognitive.azure.cn", region),
        }
    }

    fn list_scenarios(&self) -> Vec<TestScenario> {
        vec![
            TestScenario {
                id: "voices_list",
                name: "Get Voices List",
                description: "Retrieve available TTS voices",
                requires_input: false,
                input_type: None,
            },
            TestScenario {
                id: "token_exchange",
                name: "Token Exchange",
                description: "Exchange API key for short-lived token",
                requires_input: false,
                input_type: None,
            },
            TestScenario {
                id: "stt_short",
                name: "Speech-to-Text (Short Audio)",
                description: "Transcribe short audio clip",
                requires_input: true,
                input_type: Some(InputType::Audio),
            },
            TestScenario {
                id: "tts",
                name: "Text-to-Speech",
                description: "Synthesize speech from text",
                requires_input: false,
                input_type: None,
            },
        ]
    }

    async fn run_scenario(&self, scenario_id: &str, context: &TestContext) -> TestResult {
        let scenario = self
            .list_scenarios()
            .into_iter()
            .find(|s| s.id == scenario_id);

        let scenario = match scenario {
            Some(s) => s,
            None => {
                return TestResult::failure(
                    scenario_id,
                    "Unknown",
                    0,
                    format!("Unknown scenario: {}", scenario_id),
                )
            }
        };

        match scenario_id {
            "voices_list" => self.test_voices_list(context, &scenario).await,
            "token_exchange" => self.test_token_exchange(context, &scenario).await,
            "stt_short" => self.test_stt_short(context, &scenario).await,
            "tts" => self.test_tts(context, &scenario).await,
            _ => TestResult::failure(
                scenario_id,
                scenario.name,
                0,
                format!("Scenario '{}' not implemented", scenario_id),
            ),
        }
    }
}

impl SpeechService {
    async fn test_voices_list(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!("{}/cognitiveservices/voices/list", endpoint);

        let (result, duration_ms) = measure_time(async {
            let request = context.client.get(&url);
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        match response.json::<Vec<Voice>>().await {
                            Ok(voices) => Ok(format!("Retrieved {} voices", voices.len())),
                            Err(e) => Err((status.as_u16(), format!("Failed to parse response: {}", e))),
                        }
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        Err((status.as_u16(), format!("HTTP {}: {}", status, body)))
                    }
                }
                Err(e) => Err((0, format!("Request failed: {}", e))),
            }
        })
        .await;

        match result {
            Ok(details) => TestResult::success(scenario.id, scenario.name, duration_ms)
                .with_details(details),
            Err((status, error)) => {
                let mut result = TestResult::failure(scenario.id, scenario.name, duration_ms, error);
                if status > 0 {
                    result = result.with_http_status(status);
                }
                result
            }
        }
    }

    async fn test_token_exchange(
        &self,
        context: &TestContext,
        scenario: &TestScenario,
    ) -> TestResult {
        let token_endpoint = context.cloud.cognitive_token_endpoint(&context.region);

        let (result, duration_ms) = measure_time(async {
            let request = context
                .client
                .post(&token_endpoint)
                .header("Content-Length", "0");
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        match response.text().await {
                            Ok(token) => {
                                if token.len() > 100 {
                                    Ok(format!("Token received ({} chars)", token.len()))
                                } else {
                                    Err((status.as_u16(), "Invalid token received".to_string()))
                                }
                            }
                            Err(e) => Err((status.as_u16(), format!("Failed to read token: {}", e))),
                        }
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        Err((status.as_u16(), format!("HTTP {}: {}", status, body)))
                    }
                }
                Err(e) => Err((0, format!("Request failed: {}", e))),
            }
        })
        .await;

        match result {
            Ok(details) => TestResult::success(scenario.id, scenario.name, duration_ms)
                .with_details(details),
            Err((status, error)) => {
                let mut result = TestResult::failure(scenario.id, scenario.name, duration_ms, error);
                if status > 0 {
                    result = result.with_http_status(status);
                }
                result
            }
        }
    }

    async fn test_stt_short(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let input = match &context.input {
            Some(i) => i,
            None => {
                return TestResult::skipped(scenario.id, scenario.name, "No audio input provided".to_string())
            }
        };

        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!(
            "{}/speechtotext/transcriptions:transcribe?api-version=2024-11-15",
            endpoint
        );

        let (result, duration_ms) = measure_time(async {
            let form = reqwest::multipart::Form::new()
                .text("definition", r#"{"locales":["en-US"]}"#)
                .part(
                    "audio",
                    reqwest::multipart::Part::bytes(input.data.clone())
                        .file_name("audio.wav")
                        .mime_str(&input.content_type)
                        .unwrap(),
                );

            let request = context.client.post(&url).multipart(form);
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body = response.text().await.unwrap_or_default();
                        Ok(format!("Transcription received: {} chars", body.len()))
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        Err((status.as_u16(), format!("HTTP {}: {}", status, body)))
                    }
                }
                Err(e) => Err((0, format!("Request failed: {}", e))),
            }
        })
        .await;

        match result {
            Ok(details) => TestResult::success(scenario.id, scenario.name, duration_ms)
                .with_details(details),
            Err((status, error)) => {
                let mut result = TestResult::failure(scenario.id, scenario.name, duration_ms, error);
                if status > 0 {
                    result = result.with_http_status(status);
                }
                result
            }
        }
    }

    async fn test_tts(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!("{}/cognitiveservices/v1", endpoint);

        let ssml = r#"<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='en-US'>
            <voice name='en-US-JennyNeural'>
                Hello, this is a connectivity test for Azure AI Services.
            </voice>
        </speak>"#;

        let (result, duration_ms) = measure_time(async {
            let request = context
                .client
                .post(&url)
                .header("Content-Type", "application/ssml+xml")
                .header("X-Microsoft-OutputFormat", "audio-16khz-128kbitrate-mono-mp3")
                .body(ssml.to_string());
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let bytes = response.bytes().await.unwrap_or_default();
                        Ok(format!("Audio synthesized: {} bytes", bytes.len()))
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        Err((status.as_u16(), format!("HTTP {}: {}", status, body)))
                    }
                }
                Err(e) => Err((0, format!("Request failed: {}", e))),
            }
        })
        .await;

        match result {
            Ok(details) => TestResult::success(scenario.id, scenario.name, duration_ms)
                .with_details(details),
            Err((status, error)) => {
                let mut result = TestResult::failure(scenario.id, scenario.name, duration_ms, error);
                if status > 0 {
                    result = result.with_http_status(status);
                }
                result
            }
        }
    }
}
