use async_trait::async_trait;

use crate::config::Cloud;
use crate::services::{measure_time, AzureService, InputType, TestContext, TestResult, TestScenario};

/// Language Service implementation
pub struct LanguageService;

impl LanguageService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LanguageService {
    fn default() -> Self {
        Self::new()
    }
}

// These types are kept for documentation purposes and potential future use
// when we implement strongly-typed deserialization
#[allow(dead_code)]
mod _types {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize)]
    pub struct LanguageDocument {
        pub id: String,
        pub text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub language: Option<String>,
    }

    #[derive(Debug, Serialize)]
    pub struct LanguageRequest {
        pub documents: Vec<LanguageDocument>,
    }

    #[derive(Debug, Deserialize)]
    pub struct SentimentScore {
        pub positive: f64,
        pub neutral: f64,
        pub negative: f64,
    }

    #[derive(Debug, Deserialize)]
    pub struct SentimentDocument {
        pub id: String,
        pub sentiment: String,
        #[serde(rename = "confidenceScores")]
        pub confidence_scores: SentimentScore,
    }

    #[derive(Debug, Deserialize)]
    pub struct SentimentResponse {
        pub documents: Vec<SentimentDocument>,
    }

    #[derive(Debug, Deserialize)]
    pub struct DetectedLanguageDoc {
        pub id: String,
        #[serde(rename = "detectedLanguage")]
        pub detected_language: DetectedLang,
    }

    #[derive(Debug, Deserialize)]
    pub struct DetectedLang {
        pub name: String,
        pub iso6391_name: Option<String>,
        #[serde(rename = "confidenceScore")]
        pub confidence_score: f64,
    }

    #[derive(Debug, Deserialize)]
    pub struct LanguageDetectionResponse {
        pub documents: Vec<DetectedLanguageDoc>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Entity {
        pub text: String,
        pub category: String,
        #[serde(rename = "confidenceScore")]
        pub confidence_score: f64,
    }

    #[derive(Debug, Deserialize)]
    pub struct EntityDocument {
        pub id: String,
        pub entities: Vec<Entity>,
    }

    #[derive(Debug, Deserialize)]
    pub struct EntityResponse {
        pub documents: Vec<EntityDocument>,
    }

    #[derive(Debug, Deserialize)]
    pub struct KeyPhrasesDocument {
        pub id: String,
        #[serde(rename = "keyPhrases")]
        pub key_phrases: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct KeyPhrasesResponse {
        pub documents: Vec<KeyPhrasesDocument>,
    }
}

#[async_trait]
impl AzureService for LanguageService {
    fn name(&self) -> &'static str {
        "language"
    }

    fn display_name(&self) -> &'static str {
        "Language"
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
                id: "sentiment",
                name: "Sentiment Analysis",
                description: "Analyze sentiment of text",
                requires_input: false,
                input_type: Some(InputType::Text),
            },
            TestScenario {
                id: "language_detection",
                name: "Language Detection",
                description: "Detect language of text",
                requires_input: false,
                input_type: Some(InputType::Text),
            },
            TestScenario {
                id: "entities",
                name: "Named Entity Recognition",
                description: "Extract named entities from text",
                requires_input: false,
                input_type: Some(InputType::Text),
            },
            TestScenario {
                id: "key_phrases",
                name: "Key Phrase Extraction",
                description: "Extract key phrases from text",
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
            "sentiment" => self.test_sentiment(context, &scenario).await,
            "language_detection" => self.test_language_detection(context, &scenario).await,
            "entities" => self.test_entities(context, &scenario).await,
            "key_phrases" => self.test_key_phrases(context, &scenario).await,
            _ => TestResult::failure(
                scenario_id,
                scenario.name,
                0,
                format!("Scenario '{}' not implemented", scenario_id),
            ),
        }
    }
}

impl LanguageService {
    fn get_sample_text(context: &TestContext) -> String {
        context
            .input
            .as_ref()
            .and_then(|i| i.text.clone())
            .unwrap_or_else(|| {
                "The Azure AI services are excellent. Microsoft has done a great job with their cloud platform. \
                The documentation is comprehensive and the APIs are easy to use. I particularly enjoy working with \
                the cognitive services for natural language processing.".to_string()
            })
    }

    async fn test_sentiment(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!(
            "{}/language/:analyze-text?api-version=2023-04-01",
            endpoint
        );

        let text = Self::get_sample_text(context);
        let body = serde_json::json!({
            "kind": "SentimentAnalysis",
            "analysisInput": {
                "documents": [
                    {"id": "1", "text": text, "language": "en"}
                ]
            }
        });

        let (result, duration_ms) = measure_time(async {
            let request = context
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body);
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body: serde_json::Value = response.json().await.unwrap_or_default();
                        if let Some(docs) = body.get("results").and_then(|r| r.get("documents")).and_then(|d| d.as_array()) {
                            if let Some(doc) = docs.first() {
                                let sentiment = doc.get("sentiment").and_then(|s| s.as_str()).unwrap_or("unknown");
                                Ok(format!("Sentiment: {}", sentiment))
                            } else {
                                Err((status.as_u16(), "No documents in response".to_string()))
                            }
                        } else {
                            Err((status.as_u16(), "Invalid response format".to_string()))
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

    async fn test_language_detection(
        &self,
        context: &TestContext,
        scenario: &TestScenario,
    ) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!(
            "{}/language/:analyze-text?api-version=2023-04-01",
            endpoint
        );

        let text = Self::get_sample_text(context);
        let body = serde_json::json!({
            "kind": "LanguageDetection",
            "analysisInput": {
                "documents": [
                    {"id": "1", "text": text}
                ]
            }
        });

        let (result, duration_ms) = measure_time(async {
            let request = context
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body);
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body: serde_json::Value = response.json().await.unwrap_or_default();
                        if let Some(docs) = body.get("results").and_then(|r| r.get("documents")).and_then(|d| d.as_array()) {
                            if let Some(doc) = docs.first() {
                                let lang = doc.get("detectedLanguage")
                                    .and_then(|l| l.get("name"))
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("unknown");
                                Ok(format!("Detected: {}", lang))
                            } else {
                                Err((status.as_u16(), "No documents in response".to_string()))
                            }
                        } else {
                            Err((status.as_u16(), "Invalid response format".to_string()))
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

    async fn test_entities(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!(
            "{}/language/:analyze-text?api-version=2023-04-01",
            endpoint
        );

        let text = Self::get_sample_text(context);
        let body = serde_json::json!({
            "kind": "EntityRecognition",
            "analysisInput": {
                "documents": [
                    {"id": "1", "text": text, "language": "en"}
                ]
            }
        });

        let (result, duration_ms) = measure_time(async {
            let request = context
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body);
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body: serde_json::Value = response.json().await.unwrap_or_default();
                        if let Some(docs) = body.get("results").and_then(|r| r.get("documents")).and_then(|d| d.as_array()) {
                            if let Some(doc) = docs.first() {
                                let entities = doc.get("entities")
                                    .and_then(|e| e.as_array())
                                    .map(|e| e.len())
                                    .unwrap_or(0);
                                Ok(format!("Found {} entities", entities))
                            } else {
                                Err((status.as_u16(), "No documents in response".to_string()))
                            }
                        } else {
                            Err((status.as_u16(), "Invalid response format".to_string()))
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

    async fn test_key_phrases(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!(
            "{}/language/:analyze-text?api-version=2023-04-01",
            endpoint
        );

        let text = Self::get_sample_text(context);
        let body = serde_json::json!({
            "kind": "KeyPhraseExtraction",
            "analysisInput": {
                "documents": [
                    {"id": "1", "text": text, "language": "en"}
                ]
            }
        });

        let (result, duration_ms) = measure_time(async {
            let request = context
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body);
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body: serde_json::Value = response.json().await.unwrap_or_default();
                        if let Some(docs) = body.get("results").and_then(|r| r.get("documents")).and_then(|d| d.as_array()) {
                            if let Some(doc) = docs.first() {
                                let phrases = doc.get("keyPhrases")
                                    .and_then(|p| p.as_array())
                                    .map(|p| p.len())
                                    .unwrap_or(0);
                                Ok(format!("Extracted {} key phrases", phrases))
                            } else {
                                Err((status.as_u16(), "No documents in response".to_string()))
                            }
                        } else {
                            Err((status.as_u16(), "Invalid response format".to_string()))
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
}
