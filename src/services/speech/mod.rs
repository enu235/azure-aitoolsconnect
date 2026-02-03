use async_trait::async_trait;
use serde::Deserialize;

use crate::config::Cloud;
use crate::error::sanitize_error;
use crate::services::{measure_time, AzureService, InputType, TestContext, TestResult, TestScenario};

/// Minimal valid WAV file: PCM 16kHz, 16-bit, mono, ~0.1s silence (1600 samples)
/// Header: 44 bytes RIFF/WAV header + 3200 bytes of silence
const MINIMAL_WAV: &[u8] = &{
    // Total data size: 3200 bytes (1600 samples * 2 bytes each)
    const DATA_SIZE: u32 = 3200;
    const RIFF_SIZE: u32 = 4 + 24 + 8 + DATA_SIZE; // "WAVE" + fmt chunk (24) + data header (8) + data

    let mut wav = [0u8; 44 + DATA_SIZE as usize];

    // RIFF header
    wav[0] = b'R'; wav[1] = b'I'; wav[2] = b'F'; wav[3] = b'F';
    // Chunk size (little-endian)
    wav[4] = (RIFF_SIZE & 0xFF) as u8;
    wav[5] = ((RIFF_SIZE >> 8) & 0xFF) as u8;
    wav[6] = ((RIFF_SIZE >> 16) & 0xFF) as u8;
    wav[7] = ((RIFF_SIZE >> 24) & 0xFF) as u8;
    // WAVE
    wav[8] = b'W'; wav[9] = b'A'; wav[10] = b'V'; wav[11] = b'E';

    // fmt sub-chunk
    wav[12] = b'f'; wav[13] = b'm'; wav[14] = b't'; wav[15] = b' ';
    // Sub-chunk size: 16
    wav[16] = 16; wav[17] = 0; wav[18] = 0; wav[19] = 0;
    // Audio format: PCM (1)
    wav[20] = 1; wav[21] = 0;
    // Channels: 1 (mono)
    wav[22] = 1; wav[23] = 0;
    // Sample rate: 16000 (0x3E80)
    wav[24] = 0x80; wav[25] = 0x3E; wav[26] = 0; wav[27] = 0;
    // Byte rate: 32000 (16000 * 1 * 2)
    wav[28] = 0x00; wav[29] = 0x7D; wav[30] = 0; wav[31] = 0;
    // Block align: 2 (1 channel * 2 bytes)
    wav[32] = 2; wav[33] = 0;
    // Bits per sample: 16
    wav[34] = 16; wav[35] = 0;

    // data sub-chunk
    wav[36] = b'd'; wav[37] = b'a'; wav[38] = b't'; wav[39] = b'a';
    // Data size
    wav[40] = (DATA_SIZE & 0xFF) as u8;
    wav[41] = ((DATA_SIZE >> 8) & 0xFF) as u8;
    wav[42] = ((DATA_SIZE >> 16) & 0xFF) as u8;
    wav[43] = ((DATA_SIZE >> 24) & 0xFF) as u8;
    // Remaining bytes are already zero (silence)

    wav
};

/// Speech Service implementation
pub struct SpeechService;

impl SpeechService {
    pub fn new() -> Self {
        Self
    }

    /// Get audio data from user input or fall back to embedded minimal WAV
    fn get_audio_data(context: &TestContext) -> (Vec<u8>, String) {
        if let Some(input) = &context.input {
            (input.data.clone(), input.content_type.clone())
        } else {
            (MINIMAL_WAV.to_vec(), "audio/wav".to_string())
        }
    }

    /// Get the dedicated TTS endpoint for voices list and speech synthesis.
    /// Uses {region}.tts.speech.microsoft.com (not the generic cognitive services endpoint).
    fn get_tts_endpoint(region: &str, cloud: Cloud) -> String {
        match cloud {
            Cloud::Global => format!("https://{}.tts.speech.microsoft.com", region),
            Cloud::China => format!("https://{}.tts.speech.azure.cn", region),
        }
    }

    /// Get the dedicated STT endpoint for speech recognition REST API.
    /// Uses {region}.stt.speech.microsoft.com (not the generic cognitive services endpoint).
    fn get_stt_endpoint(region: &str, cloud: Cloud) -> String {
        match cloud {
            Cloud::Global => format!("https://{}.stt.speech.microsoft.com", region),
            Cloud::China => format!("https://{}.stt.speech.azure.cn", region),
        }
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
    #[serde(rename = "Name")]
    _name: String,
    #[serde(rename = "DisplayName")]
    _display_name: String,
    #[serde(rename = "LocalName")]
    _local_name: String,
    #[serde(rename = "ShortName")]
    _short_name: String,
    #[serde(rename = "Locale")]
    _locale: String,
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
            return endpoint.trim_end_matches('/').to_string();
        }
        match cloud {
            Cloud::Global => format!("https://{}.api.cognitive.microsoft.com", region),
            Cloud::China => format!("https://{}.api.cognitive.azure.cn", region),
        }
    }

    fn list_scenarios(&self) -> Vec<TestScenario> {
        vec![
            TestScenario {
                id: "endpoint_check",
                name: "Endpoint Reachability",
                description: "Verify endpoint DNS, TLS, and connectivity",
                requires_input: false,
                input_type: None,
            },
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
                name: "Speech-to-Text (Fast Transcription)",
                description: "Transcribe audio using Fast Transcription API",
                requires_input: false,
                input_type: Some(InputType::Audio),
            },
            TestScenario {
                id: "stt_rest",
                name: "Speech-to-Text (REST API)",
                description: "Transcribe audio using traditional REST API",
                requires_input: false,
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
            "endpoint_check" => self.test_endpoint_check(context, &scenario).await,
            "voices_list" => self.test_voices_list(context, &scenario).await,
            "token_exchange" => self.test_token_exchange(context, &scenario).await,
            "stt_short" => self.test_stt_short(context, &scenario).await,
            "stt_rest" => self.test_stt_rest(context, &scenario).await,
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
    async fn test_endpoint_check(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());

        let (result, duration_ms) = measure_time(async {
            match context.client.get(&endpoint).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.as_u16() < 500 {
                        Ok(format!("Endpoint reachable (HTTP {})", status))
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        Err((status.as_u16(), format!("HTTP {}: {}", status, sanitize_error(&body, status.as_u16()))))
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("dns") || msg.contains("resolve") {
                        Err((0, format!("DNS resolution failed: {}", msg)))
                    } else if msg.contains("timed out") || msg.contains("timeout") {
                        Err((0, format!("Connection timed out: {}", msg)))
                    } else if msg.contains("certificate") || msg.contains("ssl") || msg.contains("tls") {
                        Err((0, format!("TLS/SSL error: {}", msg)))
                    } else {
                        Err((0, format!("Connection failed: {}", msg)))
                    }
                }
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

    async fn test_voices_list(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let endpoint = Self::get_tts_endpoint(&context.region, context.cloud);
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
                        Err((status.as_u16(), format!("HTTP {}: {}", status, sanitize_error(&body, status.as_u16()))))
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
        let token_endpoint = context.cloud.cognitive_token_endpoint_for(
            &context.region,
            context.endpoint.as_deref(),
        );

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
                        Err((status.as_u16(), format!("HTTP {}: {}", status, sanitize_error(&body, status.as_u16()))))
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
        let (audio_data, content_type) = Self::get_audio_data(context);

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
                    reqwest::multipart::Part::bytes(audio_data)
                        .file_name("audio.wav")
                        .mime_str(&content_type)
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
                    } else if status.as_u16() == 400 {
                        let body = response.text().await.unwrap_or_default();
                        if body.contains("audio") || body.contains("InvalidRequest") || body.contains("duration") {
                            Ok(format!("Endpoint responsive (audio validation: HTTP {})", status))
                        } else {
                            Err((status.as_u16(), format!("HTTP {}: {}", status, body)))
                        }
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        Err((status.as_u16(), format!("HTTP {}: {}", status, sanitize_error(&body, status.as_u16()))))
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

    async fn test_stt_rest(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let (audio_data, content_type) = Self::get_audio_data(context);

        // Use custom endpoint for bearer token auth, otherwise use dedicated STT endpoint
        // Custom subdomain uses different API path
        let (endpoint, url) = if let Some(custom) = context.endpoint.as_deref() {
            let ep = custom.trim_end_matches('/').to_string();
            // Custom subdomain uses the newer speechtotext API
            let u = format!(
                "{}/speechtotext/speech/recognition/conversation/cognitiveservices/v1?language=en-US&format=simple",
                ep
            );
            (ep, u)
        } else {
            let ep = Self::get_stt_endpoint(&context.region, context.cloud);
            let u = format!(
                "{}/speech/recognition/conversation/cognitiveservices/v1?language=en-US&format=simple",
                ep
            );
            (ep, u)
        };

        let (result, duration_ms) = measure_time(async {
            let request = context
                .client
                .post(&url)
                .header("Content-Type", &content_type)
                .body(audio_data);
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body = response.text().await.unwrap_or_default();
                        Ok(format!("Recognition result: {} chars", body.len()))
                    } else if status.as_u16() == 400 {
                        let body = response.text().await.unwrap_or_default();
                        if body.contains("audio") || body.contains("InvalidRequest") || body.contains("duration") || body.contains("USP") {
                            Ok(format!("Endpoint responsive (audio validation: HTTP {})", status))
                        } else {
                            Err((status.as_u16(), format!("HTTP {}: {}", status, body)))
                        }
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        Err((status.as_u16(), format!("HTTP {}: {}", status, sanitize_error(&body, status.as_u16()))))
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
        // Use custom endpoint for bearer token auth, otherwise use dedicated TTS endpoint
        // Custom subdomain uses different API path
        let url = if let Some(custom) = context.endpoint.as_deref() {
            format!("{}/texttospeech/cognitiveservices/v1", custom.trim_end_matches('/'))
        } else {
            let endpoint = Self::get_tts_endpoint(&context.region, context.cloud);
            format!("{}/cognitiveservices/v1", endpoint)
        };

        let ssml = "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='en-US'><voice name='en-US-JennyNeural'>Hello, this is a connectivity test.</voice></speak>";

        let (result, duration_ms) = measure_time(async {
            let request = context
                .client
                .post(&url)
                .header("Content-Type", "application/ssml+xml")
                .header("X-Microsoft-OutputFormat", "audio-16khz-128kbitrate-mono-mp3")
                .header("User-Agent", "azure-aitoolsconnect/0.1.0")
                .body(ssml);
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let bytes = response.bytes().await.unwrap_or_default();
                        Ok(format!("Audio synthesized: {} bytes", bytes.len()))
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        Err((status.as_u16(), format!("HTTP {}: {}", status, sanitize_error(&body, status.as_u16()))))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_wav_valid_header() {
        // Verify RIFF header
        assert_eq!(&MINIMAL_WAV[0..4], b"RIFF");
        assert_eq!(&MINIMAL_WAV[8..12], b"WAVE");
        // Verify fmt chunk
        assert_eq!(&MINIMAL_WAV[12..16], b"fmt ");
        // Verify PCM format (1)
        assert_eq!(MINIMAL_WAV[20], 1);
        // Verify mono (1 channel)
        assert_eq!(MINIMAL_WAV[22], 1);
        // Verify 16kHz sample rate
        assert_eq!(u32::from_le_bytes([MINIMAL_WAV[24], MINIMAL_WAV[25], MINIMAL_WAV[26], MINIMAL_WAV[27]]), 16000);
        // Verify 16 bits per sample
        assert_eq!(u16::from_le_bytes([MINIMAL_WAV[34], MINIMAL_WAV[35]]), 16);
        // Verify data chunk
        assert_eq!(&MINIMAL_WAV[36..40], b"data");
        // Verify total size
        assert_eq!(MINIMAL_WAV.len(), 44 + 3200);
    }
}
