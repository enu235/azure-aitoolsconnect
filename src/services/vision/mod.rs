use async_trait::async_trait;

use crate::config::Cloud;
use crate::error::sanitize_error;
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

// Minimal 50x50 pixel PNG for testing (when no image provided)
// Azure Vision API requires minimum 50x50 pixels
const MINIMAL_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00, 0x32, 0x08, 0x02, 0x00, 0x00, 0x00, 0x91, 0x5D, 0x1F,
    0xE6, 0x00, 0x00, 0x00, 0x39, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0xED, 0xCE, 0x01, 0x09, 0x00,
    0x00, 0x08, 0xC0, 0xB0, 0xF7, 0x2F, 0xAD, 0x35, 0x14, 0x06, 0x0B, 0xB0, 0xA6, 0x0E, 0x4A, 0x4B,
    0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B,
    0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0x4B, 0xEB, 0x57, 0x6B, 0x01, 0x7B, 0x2B,
    0xBA, 0xC4, 0x2D, 0x11, 0x2D, 0xB9, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42,
    0x60, 0x82,
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
                description: "Extract tags, objects, and text from image",
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
            TestScenario {
                id: "smart_crops",
                name: "Smart Crops",
                description: "Generate smart-cropped thumbnails",
                requires_input: false,
                input_type: Some(InputType::Image),
            },
            TestScenario {
                id: "people_detection",
                name: "People Detection",
                description: "Detect people in image",
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
            "smart_crops" => self.test_smart_crops(context, &scenario).await,
            "people_detection" => self.test_people_detection(context, &scenario).await,
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
        // Note: Using tags,objects,read features which are available in all regions.
        // caption/denseCaptions are NOT available in some regions (e.g., swedencentral).
        let url = format!(
            "{}/computervision/imageanalysis:analyze?api-version=2024-02-01&features=tags,objects,read",
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
                        let has_objects = body.get("objectsResult").is_some();
                        let has_read = body.get("readResult").is_some();
                        Ok(format!(
                            "Analysis complete (tags: {}, objects: {}, read: {})",
                            has_tags, has_objects, has_read
                        ))
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

    async fn test_smart_crops(
        &self,
        context: &TestContext,
        scenario: &TestScenario,
    ) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        // smartCrops requires aspect ratios - using common thumbnail ratios
        let url = format!(
            "{}/computervision/imageanalysis:analyze?api-version=2024-02-01&features=smartCrops&smartCrops-aspect-ratios=1.0,1.5",
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
                        let crops = body
                            .get("smartCropsResult")
                            .and_then(|r| r.get("values"))
                            .and_then(|v| v.as_array())
                            .map(|v| v.len())
                            .unwrap_or(0);
                        Ok(format!("Smart crops complete: {} crop regions", crops))
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

    async fn test_people_detection(
        &self,
        context: &TestContext,
        scenario: &TestScenario,
    ) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!(
            "{}/computervision/imageanalysis:analyze?api-version=2024-02-01&features=people",
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
                        let people = body
                            .get("peopleResult")
                            .and_then(|r| r.get("values"))
                            .and_then(|v| v.as_array())
                            .map(|v| v.len())
                            .unwrap_or(0);
                        Ok(format!("People detection complete: {} people found", people))
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
