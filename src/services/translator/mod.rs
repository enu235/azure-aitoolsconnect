use async_trait::async_trait;

use crate::config::Cloud;
use crate::error::sanitize_error;
use crate::services::{measure_time, AzureService, InputType, TestContext, TestResult, TestScenario};

/// Translator Service implementation
pub struct TranslatorService;

impl TranslatorService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TranslatorService {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, serde::Serialize)]
struct TranslateRequest {
    #[serde(rename = "Text")]
    text: String,
}

#[derive(Debug, serde::Deserialize)]
struct DetectResponse {
    language: String,
    #[serde(rename = "score")]
    _score: f64,
}

#[derive(Debug, serde::Deserialize)]
struct TranslationText {
    text: String,
    to: String,
}

#[derive(Debug, serde::Deserialize)]
struct TranslateResponse {
    translations: Vec<TranslationText>,
}

#[async_trait]
impl AzureService for TranslatorService {
    fn name(&self) -> &'static str {
        "translator"
    }

    fn display_name(&self) -> &'static str {
        "Translator"
    }

    fn get_endpoint(&self, _region: &str, cloud: Cloud, custom_endpoint: Option<&str>) -> String {
        if let Some(endpoint) = custom_endpoint {
            // Custom subdomain uses different API path prefix
            return format!("{}/translator/text/v3.0", endpoint.trim_end_matches('/'));
        }
        match cloud {
            Cloud::Global => "https://api.cognitive.microsofttranslator.com".to_string(),
            Cloud::China => "https://api.translator.azure.cn".to_string(),
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
                id: "languages",
                name: "Get Languages",
                description: "Get list of supported languages (no auth required)",
                requires_input: false,
                input_type: None,
            },
            TestScenario {
                id: "detect",
                name: "Detect Language",
                description: "Detect language of text",
                requires_input: false,
                input_type: Some(InputType::Text),
            },
            TestScenario {
                id: "translate",
                name: "Translate Text",
                description: "Translate text between languages",
                requires_input: false,
                input_type: Some(InputType::Text),
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
            "languages" => self.test_languages(context, &scenario).await,
            "detect" => self.test_detect(context, &scenario).await,
            "translate" => self.test_translate(context, &scenario).await,
            _ => TestResult::failure(
                scenario_id,
                scenario.name,
                0,
                format!("Scenario '{}' not implemented", scenario_id),
            ),
        }
    }
}

impl TranslatorService {
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

    async fn test_languages(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        // Languages endpoint is public and doesn't require auth
        // Always use the global endpoint for this, as custom subdomain may not support unauthenticated requests
        let url = match context.cloud {
            Cloud::Global => "https://api.cognitive.microsofttranslator.com/languages?api-version=3.0".to_string(),
            Cloud::China => "https://api.translator.azure.cn/languages?api-version=3.0".to_string(),
        };

        let (result, duration_ms) = measure_time(async {
            // Languages endpoint doesn't require authentication - use plain request
            match context.client.get(&url).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body: serde_json::Value = response.json().await.unwrap_or_default();
                        let translation_count = body
                            .get("translation")
                            .and_then(|t| t.as_object())
                            .map(|o| o.len())
                            .unwrap_or(0);
                        Ok(format!("{} translation languages available", translation_count))
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

    async fn test_detect(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!("{}/detect?api-version=3.0", endpoint);

        // Use provided text or default sample
        let text = context
            .input
            .as_ref()
            .and_then(|i| i.text.clone())
            .unwrap_or_else(|| "Hello, how are you today?".to_string());

        let body = vec![TranslateRequest { text }];

        let (result, duration_ms) = measure_time(async {
            let request = context
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body);
            let request = context.credentials.apply_to_request(request);
            // Add region header for global endpoint
            let request = request.header("Ocp-Apim-Subscription-Region", &context.region);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        match response.json::<Vec<DetectResponse>>().await {
                            Ok(results) => {
                                if let Some(first) = results.first() {
                                    Ok(format!("Detected language: {}", first.language))
                                } else {
                                    Err((status.as_u16(), "Empty response".to_string()))
                                }
                            }
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

    async fn test_translate(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!("{}/translate?api-version=3.0&to=es", endpoint);

        // Use provided text or default sample
        let text = context
            .input
            .as_ref()
            .and_then(|i| i.text.clone())
            .unwrap_or_else(|| "Hello, this is a connectivity test.".to_string());

        let body = vec![TranslateRequest { text }];

        let (result, duration_ms) = measure_time(async {
            let request = context
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body);
            let request = context.credentials.apply_to_request(request);
            // Add region header for global endpoint
            let request = request.header("Ocp-Apim-Subscription-Region", &context.region);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        match response.json::<Vec<TranslateResponse>>().await {
                            Ok(results) => {
                                if let Some(first) = results.first() {
                                    if let Some(translation) = first.translations.first() {
                                        Ok(format!(
                                            "Translated to {}: {}",
                                            translation.to,
                                            translation.text.chars().take(50).collect::<String>()
                                        ))
                                    } else {
                                        Err((status.as_u16(), "No translations returned".to_string()))
                                    }
                                } else {
                                    Err((status.as_u16(), "Empty response".to_string()))
                                }
                            }
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
}
