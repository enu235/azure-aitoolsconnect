use async_trait::async_trait;

use crate::config::Cloud;
use crate::services::{measure_time, AzureService, InputType, TestContext, TestResult, TestScenario};

/// Vision Service implementation
pub struct VisionService;

impl VisionService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for VisionService {
    fn default() -> Self {
        Self::new()
    }
}

// Minimal 1x1 pixel PNG for testing (when no image provided)
const MINIMAL_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xFF, 0xFF, 0x3F,
    0x00, 0x05, 0xFE, 0x02, 0xFE, 0xDC, 0xCC, 0x59, 0xE7, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

#[async_trait]
impl AzureService for VisionService {
    fn name(&self) -> &'static str {
        "vision"
    }

    fn display_name(&self) -> &'static str {
        "Vision"
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
                id: "analyze_image",
                name: "Analyze Image",
                description: "Extract tags, categories, and descriptions from image",
                requires_input: false,
                input_type: Some(InputType::Image),
            },
            TestScenario {
                id: "read_text",
                name: "Read Text (OCR)",
                description: "Extract text from image using OCR",
                requires_input: false,
                input_type: Some(InputType::Image),
            },
            TestScenario {
                id: "detect_objects",
                name: "Detect Objects",
                description: "Detect and locate objects in image",
                requires_input: false,
                input_type: Some(InputType::Image),
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
            "analyze_image" => self.test_analyze_image(context, &scenario).await,
            "read_text" => self.test_read_text(context, &scenario).await,
            "detect_objects" => self.test_detect_objects(context, &scenario).await,
            _ => TestResult::failure(
                scenario_id,
                scenario.name,
                0,
                format!("Scenario '{}' not implemented", scenario_id),
            ),
        }
    }
}

impl VisionService {
    fn get_image_data(context: &TestContext) -> (Vec<u8>, String) {
        if let Some(input) = &context.input {
            (input.data.clone(), input.content_type.clone())
        } else {
            // Use embedded minimal PNG for connectivity testing
            (MINIMAL_PNG.to_vec(), "image/png".to_string())
        }
    }

    async fn test_analyze_image(
        &self,
        context: &TestContext,
        scenario: &TestScenario,
    ) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!(
            "{}/computervision/imageanalysis:analyze?api-version=2024-02-01&features=tags,caption,read",
            endpoint
        );

        let (image_data, content_type) = Self::get_image_data(context);

        let (result, duration_ms) = measure_time(async {
            let request = context
                .client
                .post(&url)
                .header("Content-Type", &content_type)
                .body(image_data);
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body: serde_json::Value = response.json().await.unwrap_or_default();
                        let has_tags = body.get("tagsResult").is_some();
                        let has_caption = body.get("captionResult").is_some();
                        Ok(format!(
                            "Analysis complete (tags: {}, caption: {})",
                            has_tags, has_caption
                        ))
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

    async fn test_read_text(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!(
            "{}/computervision/imageanalysis:analyze?api-version=2024-02-01&features=read",
            endpoint
        );

        let (image_data, content_type) = Self::get_image_data(context);

        let (result, duration_ms) = measure_time(async {
            let request = context
                .client
                .post(&url)
                .header("Content-Type", &content_type)
                .body(image_data);
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body: serde_json::Value = response.json().await.unwrap_or_default();
                        let blocks = body
                            .get("readResult")
                            .and_then(|r| r.get("blocks"))
                            .and_then(|b| b.as_array())
                            .map(|b| b.len())
                            .unwrap_or(0);
                        Ok(format!("Read complete: {} text blocks found", blocks))
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

    async fn test_detect_objects(
        &self,
        context: &TestContext,
        scenario: &TestScenario,
    ) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!(
            "{}/computervision/imageanalysis:analyze?api-version=2024-02-01&features=objects",
            endpoint
        );

        let (image_data, content_type) = Self::get_image_data(context);

        let (result, duration_ms) = measure_time(async {
            let request = context
                .client
                .post(&url)
                .header("Content-Type", &content_type)
                .body(image_data);
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body: serde_json::Value = response.json().await.unwrap_or_default();
                        let objects = body
                            .get("objectsResult")
                            .and_then(|r| r.get("values"))
                            .and_then(|v| v.as_array())
                            .map(|v| v.len())
                            .unwrap_or(0);
                        Ok(format!("Detection complete: {} objects found", objects))
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
