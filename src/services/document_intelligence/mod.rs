use async_trait::async_trait;
use std::time::Duration;

use crate::config::Cloud;
use crate::error::sanitize_error;
use crate::services::{measure_time, AzureService, InputType, TestContext, TestResult, TestScenario};

/// Document Intelligence Service implementation
pub struct DocumentIntelligenceService;

impl DocumentIntelligenceService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DocumentIntelligenceService {
    fn default() -> Self {
        Self::new()
    }
}

// Minimal PDF for testing (when no document provided)
// This is a tiny valid PDF with one blank page
const MINIMAL_PDF: &[u8] = b"%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>
endobj
xref
0 4
0000000000 65535 f
0000000009 00000 n
0000000058 00000 n
0000000115 00000 n
trailer
<< /Size 4 /Root 1 0 R >>
startxref
196
%%EOF";

#[async_trait]
impl AzureService for DocumentIntelligenceService {
    fn name(&self) -> &'static str {
        "document_intelligence"
    }

    fn display_name(&self) -> &'static str {
        "Document Intelligence"
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
                id: "layout",
                name: "Layout Analysis",
                description: "Extract layout and structure from document",
                requires_input: false,
                input_type: Some(InputType::Document),
            },
            TestScenario {
                id: "read",
                name: "Read (OCR)",
                description: "Extract text from document using OCR",
                requires_input: false,
                input_type: Some(InputType::Document),
            },
            // Note: prebuilt-document model was retired in 2024.
            // Key-value extraction is now available via prebuilt-layout with keyValuePairs feature.
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
            "layout" => self.test_layout(context, &scenario).await,
            "read" => self.test_read(context, &scenario).await,
            _ => TestResult::failure(
                scenario_id,
                scenario.name,
                0,
                format!("Scenario '{}' not implemented", scenario_id),
            ),
        }
    }
}

impl DocumentIntelligenceService {
    fn get_document_data(context: &TestContext) -> (Vec<u8>, String) {
        if let Some(input) = &context.input {
            (input.data.clone(), input.content_type.clone())
        } else {
            // Use embedded minimal PDF for connectivity testing
            (MINIMAL_PDF.to_vec(), "application/pdf".to_string())
        }
    }

    async fn analyze_document(
        &self,
        context: &TestContext,
        model_id: &str,
        scenario: &TestScenario,
    ) -> TestResult {
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let url = format!(
            "{}/documentintelligence/documentModels/{}:analyze?api-version=2024-11-30",
            endpoint, model_id
        );

        let (document_data, content_type) = Self::get_document_data(context);

        let (result, duration_ms) = measure_time(async {
            // Start the analysis operation
            let request = context
                .client
                .post(&url)
                .header("Content-Type", &content_type)
                .body(document_data);
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();

                    // Document Intelligence returns 202 Accepted for async operations
                    if status == reqwest::StatusCode::ACCEPTED {
                        // Get the operation location
                        if let Some(operation_location) = response
                            .headers()
                            .get("operation-location")
                            .and_then(|v| v.to_str().ok())
                        {
                            // Poll for completion
                            self.poll_operation(context, operation_location).await
                        } else {
                            Err((status.as_u16(), "No operation-location header in response".to_string()))
                        }
                    } else if status.is_success() {
                        // Some operations might return synchronously
                        let body: serde_json::Value = response.json().await.unwrap_or_default();
                        Ok(format!("Analysis complete: {:?}", body.get("status")))
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

    async fn poll_operation(
        &self,
        context: &TestContext,
        operation_url: &str,
    ) -> Result<String, (u16, String)> {
        // Poll for up to 30 seconds
        let max_attempts = 30;
        let poll_interval = Duration::from_secs(1);

        for _ in 0..max_attempts {
            tokio::time::sleep(poll_interval).await;

            let request = context.client.get(operation_url);
            let request = context.credentials.apply_to_request(request);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body: serde_json::Value = response.json().await.unwrap_or_default();

                        if let Some(op_status) = body.get("status").and_then(|s| s.as_str()) {
                            match op_status {
                                "succeeded" => {
                                    let pages = body
                                        .get("analyzeResult")
                                        .and_then(|r| r.get("pages"))
                                        .and_then(|p| p.as_array())
                                        .map(|p| p.len())
                                        .unwrap_or(0);
                                    return Ok(format!("Analysis succeeded: {} pages processed", pages));
                                }
                                "failed" => {
                                    let error = body
                                        .get("error")
                                        .and_then(|e| e.get("message"))
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("Unknown error");
                                    return Err((status.as_u16(), format!("Analysis failed: {}", error)));
                                }
                                "running" | "notStarted" => {
                                    // Continue polling
                                }
                                _ => {
                                    return Err((status.as_u16(), format!("Unknown status: {}", op_status)));
                                }
                            }
                        }
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        return Err((status.as_u16(), format!("HTTP {}: {}", status, body)));
                    }
                }
                Err(e) => {
                    return Err((0, format!("Poll request failed: {}", e)));
                }
            }
        }

        Err((0, "Operation timed out after 30 seconds".to_string()))
    }

    async fn test_layout(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        self.analyze_document(context, "prebuilt-layout", scenario).await
    }

    async fn test_read(&self, context: &TestContext, scenario: &TestScenario) -> TestResult {
        self.analyze_document(context, "prebuilt-read", scenario).await
    }
}
