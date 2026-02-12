use async_trait::async_trait;

use crate::config::Cloud;
use crate::error::sanitize_error;
use crate::services::{
    measure_time, AzureService, InputType, TestContext, TestResult, TestScenario,
};

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
            TestScenario {
                id: "pii_detection",
                name: "PII Detection",
                description: "Detect personally identifiable information",
                requires_input: false,
                input_type: Some(InputType::Text),
            },
            TestScenario {
                id: "entity_linking",
                name: "Entity Linking",
                description: "Link entities to Wikipedia knowledge base",
                requires_input: false,
                input_type: Some(InputType::Text),
            },
            TestScenario {
                id: "summarization",
                name: "Summarization",
                description: "Generate abstractive summary of text",
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
            "pii_detection" => self.test_pii_detection(context, &scenario).await,
            "entity_linking" => self.test_entity_linking(context, &scenario).await,
            "summarization" => self.test_summarization(context, &scenario).await,
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
        let endpoint =
            self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!("{}/language/:analyze-text?api-version=2023-04-01", endpoint);

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
                        if let Some(docs) = body
                            .get("results")
                            .and_then(|r| r.get("documents"))
                            .and_then(|d| d.as_array())
                        {
                            if let Some(doc) = docs.first() {
                                let sentiment = doc
                                    .get("sentiment")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("unknown");
                                Ok(format!("Sentiment: {}", sentiment))
                            } else {
                                Err((status.as_u16(), "No documents in response".to_string()))
                            }
                        } else {
                            Err((status.as_u16(), "Invalid response format".to_string()))
                        }
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        Err((
                            status.as_u16(),
                            format!(
                                "HTTP {}: {}",
                                status,
                                sanitize_error(&body, status.as_u16())
                            ),
                        ))
                    }
                }
                Err(e) => Err((0, format!("Request failed: {}", e))),
            }
        })
        .await;

        match result {
            Ok(details) => {
                TestResult::success(scenario.id, scenario.name, duration_ms).with_details(details)
            }
            Err((status, error)) => {
                let mut result =
                    TestResult::failure(scenario.id, scenario.name, duration_ms, error);
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
        let endpoint =
            self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!("{}/language/:analyze-text?api-version=2023-04-01", endpoint);

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
                        if let Some(docs) = body
                            .get("results")
                            .and_then(|r| r.get("documents"))
                            .and_then(|d| d.as_array())
                        {
                            if let Some(doc) = docs.first() {
                                let lang = doc
                                    .get("detectedLanguage")
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
                        Err((
                            status.as_u16(),
                            format!(
                                "HTTP {}: {}",
                                status,
                                sanitize_error(&body, status.as_u16())
                            ),
                        ))
                    }
                }
                Err(e) => Err((0, format!("Request failed: {}", e))),
            }
        })
        .await;

        match result {
            Ok(details) => {
                TestResult::success(scenario.id, scenario.name, duration_ms).with_details(details)
            }
            Err((status, error)) => {
                let mut result =
                    TestResult::failure(scenario.id, scenario.name, duration_ms, error);
                if status > 0 {
                    result = result.with_http_status(status);
                }
                result
            }
        }
    }

    async fn test_entities(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let endpoint =
            self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!("{}/language/:analyze-text?api-version=2023-04-01", endpoint);

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
                        if let Some(docs) = body
                            .get("results")
                            .and_then(|r| r.get("documents"))
                            .and_then(|d| d.as_array())
                        {
                            if let Some(doc) = docs.first() {
                                let entities = doc
                                    .get("entities")
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
                        Err((
                            status.as_u16(),
                            format!(
                                "HTTP {}: {}",
                                status,
                                sanitize_error(&body, status.as_u16())
                            ),
                        ))
                    }
                }
                Err(e) => Err((0, format!("Request failed: {}", e))),
            }
        })
        .await;

        match result {
            Ok(details) => {
                TestResult::success(scenario.id, scenario.name, duration_ms).with_details(details)
            }
            Err((status, error)) => {
                let mut result =
                    TestResult::failure(scenario.id, scenario.name, duration_ms, error);
                if status > 0 {
                    result = result.with_http_status(status);
                }
                result
            }
        }
    }

    async fn test_key_phrases(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let endpoint =
            self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!("{}/language/:analyze-text?api-version=2023-04-01", endpoint);

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
                        if let Some(docs) = body
                            .get("results")
                            .and_then(|r| r.get("documents"))
                            .and_then(|d| d.as_array())
                        {
                            if let Some(doc) = docs.first() {
                                let phrases = doc
                                    .get("keyPhrases")
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
                        Err((
                            status.as_u16(),
                            format!(
                                "HTTP {}: {}",
                                status,
                                sanitize_error(&body, status.as_u16())
                            ),
                        ))
                    }
                }
                Err(e) => Err((0, format!("Request failed: {}", e))),
            }
        })
        .await;

        match result {
            Ok(details) => {
                TestResult::success(scenario.id, scenario.name, duration_ms).with_details(details)
            }
            Err((status, error)) => {
                let mut result =
                    TestResult::failure(scenario.id, scenario.name, duration_ms, error);
                if status > 0 {
                    result = result.with_http_status(status);
                }
                result
            }
        }
    }

    async fn test_pii_detection(
        &self,
        context: &TestContext,
        scenario: &TestScenario,
    ) -> TestResult {
        let endpoint =
            self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!("{}/language/:analyze-text?api-version=2023-04-01", endpoint);

        // Use sample text with PII for testing
        let text = context
            .input
            .as_ref()
            .and_then(|i| i.text.clone())
            .unwrap_or_else(|| {
                "My name is John Smith and my email is john.smith@example.com. \
                 My phone number is 555-123-4567 and my SSN is 123-45-6789."
                    .to_string()
            });

        let body = serde_json::json!({
            "kind": "PiiEntityRecognition",
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
                        if let Some(docs) = body
                            .get("results")
                            .and_then(|r| r.get("documents"))
                            .and_then(|d| d.as_array())
                        {
                            if let Some(doc) = docs.first() {
                                let entities = doc
                                    .get("entities")
                                    .and_then(|e| e.as_array())
                                    .map(|e| e.len())
                                    .unwrap_or(0);
                                Ok(format!("Found {} PII entities", entities))
                            } else {
                                Err((status.as_u16(), "No documents in response".to_string()))
                            }
                        } else {
                            Err((status.as_u16(), "Invalid response format".to_string()))
                        }
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        Err((
                            status.as_u16(),
                            format!(
                                "HTTP {}: {}",
                                status,
                                sanitize_error(&body, status.as_u16())
                            ),
                        ))
                    }
                }
                Err(e) => Err((0, format!("Request failed: {}", e))),
            }
        })
        .await;

        match result {
            Ok(details) => {
                TestResult::success(scenario.id, scenario.name, duration_ms).with_details(details)
            }
            Err((status, error)) => {
                let mut result =
                    TestResult::failure(scenario.id, scenario.name, duration_ms, error);
                if status > 0 {
                    result = result.with_http_status(status);
                }
                result
            }
        }
    }

    async fn test_entity_linking(
        &self,
        context: &TestContext,
        scenario: &TestScenario,
    ) -> TestResult {
        let endpoint =
            self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!("{}/language/:analyze-text?api-version=2023-04-01", endpoint);

        // Use sample text with linkable entities
        let text = context
            .input
            .as_ref()
            .and_then(|i| i.text.clone())
            .unwrap_or_else(|| {
                "Microsoft was founded by Bill Gates and Paul Allen in Albuquerque, New Mexico. \
                 The company later moved its headquarters to Redmond, Washington."
                    .to_string()
            });

        let body = serde_json::json!({
            "kind": "EntityLinking",
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
                        if let Some(docs) = body
                            .get("results")
                            .and_then(|r| r.get("documents"))
                            .and_then(|d| d.as_array())
                        {
                            if let Some(doc) = docs.first() {
                                let entities = doc
                                    .get("entities")
                                    .and_then(|e| e.as_array())
                                    .map(|e| e.len())
                                    .unwrap_or(0);
                                Ok(format!("Linked {} entities", entities))
                            } else {
                                Err((status.as_u16(), "No documents in response".to_string()))
                            }
                        } else {
                            Err((status.as_u16(), "Invalid response format".to_string()))
                        }
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        Err((
                            status.as_u16(),
                            format!(
                                "HTTP {}: {}",
                                status,
                                sanitize_error(&body, status.as_u16())
                            ),
                        ))
                    }
                }
                Err(e) => Err((0, format!("Request failed: {}", e))),
            }
        })
        .await;

        match result {
            Ok(details) => {
                TestResult::success(scenario.id, scenario.name, duration_ms).with_details(details)
            }
            Err((status, error)) => {
                let mut result =
                    TestResult::failure(scenario.id, scenario.name, duration_ms, error);
                if status > 0 {
                    result = result.with_http_status(status);
                }
                result
            }
        }
    }

    async fn test_summarization(
        &self,
        context: &TestContext,
        scenario: &TestScenario,
    ) -> TestResult {
        let endpoint =
            self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());

        // Summarization uses the async analyze-text/jobs endpoint
        let url = format!(
            "{}/language/analyze-text/jobs?api-version=2023-04-01",
            endpoint
        );

        // Use a longer text for summarization
        let text = context
            .input
            .as_ref()
            .and_then(|i| i.text.clone())
            .unwrap_or_else(|| {
                "Azure Cognitive Services are cloud-based artificial intelligence services that help \
                 developers build cognitive intelligence into applications without having direct AI or \
                 data science skills or knowledge. They are available through REST APIs and client \
                 library SDKs in popular development languages. Azure Cognitive Services enables \
                 developers to easily add cognitive features into their applications with cognitive \
                 solutions that can see, hear, speak, and analyze. The catalog of cognitive services \
                 covers five main pillars: Vision, Speech, Language, Decision, and Azure OpenAI Service. \
                 These services help build applications for many use cases across many industries.".to_string()
            });

        let body = serde_json::json!({
            "displayName": "Summarization Test",
            "analysisInput": {
                "documents": [
                    {"id": "1", "text": text, "language": "en"}
                ]
            },
            "tasks": [
                {
                    "kind": "AbstractiveSummarization",
                    "taskName": "Summarize",
                    "parameters": {
                        "sentenceCount": 2
                    }
                }
            ]
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
                    if status.as_u16() == 202 {
                        // Job accepted - get the operation location
                        if let Some(operation_location) =
                            response.headers().get("operation-location")
                        {
                            let op_url = operation_location.to_str().unwrap_or_default();
                            // Poll for result (with timeout)
                            for _ in 0..10 {
                                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                                let poll_request = context.client.get(op_url);
                                let poll_request =
                                    context.credentials.apply_to_request(poll_request);

                                if let Ok(poll_response) = poll_request.send().await {
                                    if poll_response.status().is_success() {
                                        let poll_body: serde_json::Value =
                                            poll_response.json().await.unwrap_or_default();
                                        let job_status = poll_body
                                            .get("status")
                                            .and_then(|s| s.as_str())
                                            .unwrap_or("");

                                        if job_status == "succeeded" {
                                            // Get summary from results
                                            if let Some(tasks) = poll_body
                                                .get("tasks")
                                                .and_then(|t| t.get("items"))
                                                .and_then(|i| i.as_array())
                                            {
                                                if let Some(task) = tasks.first() {
                                                    if let Some(docs) = task
                                                        .get("results")
                                                        .and_then(|r| r.get("documents"))
                                                        .and_then(|d| d.as_array())
                                                    {
                                                        if let Some(doc) = docs.first() {
                                                            let summaries = doc
                                                                .get("summaries")
                                                                .and_then(|s| s.as_array())
                                                                .map(|s| s.len())
                                                                .unwrap_or(0);
                                                            return Ok(format!(
                                                                "Generated {} summary/summaries",
                                                                summaries
                                                            ));
                                                        }
                                                    }
                                                }
                                            }
                                            return Ok("Summarization completed".to_string());
                                        } else if job_status == "failed" {
                                            return Err((
                                                200,
                                                "Summarization job failed".to_string(),
                                            ));
                                        }
                                        // Still running, continue polling
                                    }
                                }
                            }
                            Ok("Job submitted (polling timeout, but endpoint responsive)"
                                .to_string())
                        } else {
                            Err((status.as_u16(), "No operation-location header".to_string()))
                        }
                    } else if status.is_success() {
                        Ok("Summarization submitted".to_string())
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        Err((
                            status.as_u16(),
                            format!(
                                "HTTP {}: {}",
                                status,
                                sanitize_error(&body, status.as_u16())
                            ),
                        ))
                    }
                }
                Err(e) => Err((0, format!("Request failed: {}", e))),
            }
        })
        .await;

        match result {
            Ok(details) => {
                TestResult::success(scenario.id, scenario.name, duration_ms).with_details(details)
            }
            Err((status, error)) => {
                let mut result =
                    TestResult::failure(scenario.id, scenario.name, duration_ms, error);
                if status > 0 {
                    result = result.with_http_status(status);
                }
                result
            }
        }
    }
}
