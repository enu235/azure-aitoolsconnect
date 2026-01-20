pub mod document_intelligence;
pub mod language;
pub mod speech;
pub mod translator;
pub mod vision;

use crate::auth::Credentials;
use crate::config::Cloud;
use crate::error::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Test scenario definition
#[derive(Debug, Clone)]
pub struct TestScenario {
    /// Unique identifier for the scenario
    pub id: &'static str,
    /// Human-readable name
    pub name: &'static str,
    /// Description of what this scenario tests
    pub description: &'static str,
    /// Whether this scenario requires input data (audio, image, document)
    pub requires_input: bool,
    /// Type of input required (if any)
    pub input_type: Option<InputType>,
}

/// Type of input file required
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputType {
    Audio,
    Image,
    Document,
    Text,
}

impl std::fmt::Display for InputType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputType::Audio => write!(f, "audio"),
            InputType::Image => write!(f, "image"),
            InputType::Document => write!(f, "document"),
            InputType::Text => write!(f, "text"),
        }
    }
}

/// Result of a single test scenario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Scenario ID
    pub scenario_id: String,
    /// Scenario name
    pub scenario_name: String,
    /// Whether the test passed
    pub success: bool,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Error message if failed
    pub error: Option<String>,
    /// Additional details/diagnostics
    pub details: Option<String>,
    /// HTTP status code if applicable
    pub http_status: Option<u16>,
}

impl TestResult {
    pub fn success(scenario_id: &str, scenario_name: &str, duration_ms: u64) -> Self {
        Self {
            scenario_id: scenario_id.to_string(),
            scenario_name: scenario_name.to_string(),
            success: true,
            duration_ms,
            error: None,
            details: None,
            http_status: None,
        }
    }

    pub fn failure(
        scenario_id: &str,
        scenario_name: &str,
        duration_ms: u64,
        error: String,
    ) -> Self {
        Self {
            scenario_id: scenario_id.to_string(),
            scenario_name: scenario_name.to_string(),
            success: false,
            duration_ms,
            error: Some(error),
            details: None,
            http_status: None,
        }
    }

    pub fn with_details(mut self, details: String) -> Self {
        self.details = Some(details);
        self
    }

    pub fn with_http_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    pub fn skipped(scenario_id: &str, scenario_name: &str, reason: String) -> Self {
        Self {
            scenario_id: scenario_id.to_string(),
            scenario_name: scenario_name.to_string(),
            success: false,
            duration_ms: 0,
            error: Some(format!("Skipped: {}", reason)),
            details: None,
            http_status: None,
        }
    }
}

/// Results from testing a complete service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceTestResults {
    /// Service name
    pub service_name: String,
    /// Service endpoint that was tested
    pub endpoint: String,
    /// Individual test results
    pub results: Vec<TestResult>,
    /// Total duration in milliseconds
    pub total_duration_ms: u64,
}

impl ServiceTestResults {
    pub fn passed(&self) -> usize {
        self.results.iter().filter(|r| r.success).count()
    }

    pub fn failed(&self) -> usize {
        self.results.iter().filter(|r| !r.success).count()
    }

    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.success)
    }
}

/// Input data for tests
#[derive(Debug, Clone)]
pub struct TestInput {
    /// Raw data bytes
    pub data: Vec<u8>,
    /// Content type (MIME type)
    pub content_type: String,
    /// File name (if applicable)
    pub file_name: Option<String>,
    /// Text input (for text-based tests)
    pub text: Option<String>,
}

impl TestInput {
    pub fn text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            data: text.as_bytes().to_vec(),
            content_type: "text/plain".to_string(),
            file_name: None,
            text: Some(text),
        }
    }

    pub fn audio(data: Vec<u8>, content_type: &str) -> Self {
        Self {
            data,
            content_type: content_type.to_string(),
            file_name: None,
            text: None,
        }
    }

    pub fn image(data: Vec<u8>, content_type: &str) -> Self {
        Self {
            data,
            content_type: content_type.to_string(),
            file_name: None,
            text: None,
        }
    }

    pub fn document(data: Vec<u8>, content_type: &str) -> Self {
        Self {
            data,
            content_type: content_type.to_string(),
            file_name: None,
            text: None,
        }
    }
}

/// Test context passed to service implementations
pub struct TestContext {
    /// HTTP client
    pub client: Client,
    /// Credentials for authentication
    pub credentials: Credentials,
    /// Request timeout
    pub timeout: Duration,
    /// Cloud environment
    pub cloud: Cloud,
    /// Region
    pub region: String,
    /// Optional custom endpoint
    pub endpoint: Option<String>,
    /// Optional input data
    pub input: Option<TestInput>,
    /// Verbose output
    pub verbose: bool,
}

impl TestContext {
    pub fn new(
        credentials: Credentials,
        cloud: Cloud,
        region: String,
        timeout: Duration,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| crate::error::AppError::Network(e.to_string()))?;

        Ok(Self {
            client,
            credentials,
            timeout,
            cloud,
            region,
            endpoint: None,
            input: None,
            verbose: false,
        })
    }

    pub fn with_endpoint(mut self, endpoint: Option<String>) -> Self {
        self.endpoint = endpoint;
        self
    }

    pub fn with_input(mut self, input: Option<TestInput>) -> Self {
        self.input = input;
        self
    }

    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }
}

/// Trait for Azure AI Service implementations
#[async_trait]
pub trait AzureService: Send + Sync {
    /// Service name (e.g., "speech", "translator")
    fn name(&self) -> &'static str;

    /// Human-readable display name
    fn display_name(&self) -> &'static str;

    /// Get the base endpoint URL for this service
    fn get_endpoint(&self, region: &str, cloud: Cloud, custom_endpoint: Option<&str>) -> String;

    /// List available test scenarios
    fn list_scenarios(&self) -> Vec<TestScenario>;

    /// Run a specific test scenario
    async fn run_scenario(
        &self,
        scenario_id: &str,
        context: &TestContext,
    ) -> TestResult;

    /// Run all enabled scenarios
    async fn run_all_scenarios(
        &self,
        context: &TestContext,
        enabled_scenarios: Option<&[String]>,
    ) -> ServiceTestResults {
        let scenarios = self.list_scenarios();
        let endpoint = self.get_endpoint(&context.region, context.cloud, context.endpoint.as_deref());
        let start = Instant::now();
        let mut results = Vec::new();

        for scenario in &scenarios {
            // Check if this scenario is in the enabled list
            if let Some(enabled) = enabled_scenarios {
                if !enabled.iter().any(|s| s == scenario.id) {
                    continue;
                }
            }

            // Check if we have required input
            if scenario.requires_input {
                if context.input.is_none() {
                    results.push(TestResult::skipped(
                        scenario.id,
                        scenario.name,
                        format!("Requires {} input", scenario.input_type.map(|t| t.to_string()).unwrap_or_default()),
                    ));
                    continue;
                }
            }

            let result = self.run_scenario(scenario.id, context).await;
            results.push(result);
        }

        ServiceTestResults {
            service_name: self.display_name().to_string(),
            endpoint,
            results,
            total_duration_ms: start.elapsed().as_millis() as u64,
        }
    }
}

/// Helper function to measure execution time
pub async fn measure_time<F, T>(f: F) -> (T, u64)
where
    F: std::future::Future<Output = T>,
{
    let start = Instant::now();
    let result = f.await;
    let duration_ms = start.elapsed().as_millis() as u64;
    (result, duration_ms)
}

/// Get all available services
pub fn get_all_services() -> Vec<Box<dyn AzureService>> {
    vec![
        Box::new(speech::SpeechService::new()),
        Box::new(translator::TranslatorService::new()),
        Box::new(language::LanguageService::new()),
        Box::new(vision::VisionService::new()),
        Box::new(document_intelligence::DocumentIntelligenceService::new()),
    ]
}

/// Get a service by name
pub fn get_service(name: &str) -> Option<Box<dyn AzureService>> {
    match name.to_lowercase().as_str() {
        "speech" => Some(Box::new(speech::SpeechService::new())),
        "translator" => Some(Box::new(translator::TranslatorService::new())),
        "language" => Some(Box::new(language::LanguageService::new())),
        "vision" => Some(Box::new(vision::VisionService::new())),
        "document_intelligence" | "document-intelligence" | "documentintelligence" => {
            Some(Box::new(document_intelligence::DocumentIntelligenceService::new()))
        }
        _ => None,
    }
}
