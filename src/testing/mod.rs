use crate::auth::Credentials;
use crate::config::{AuthMethod, Cloud, Config};
use crate::error::{AppError, Result};
use crate::output::TestReport;
use crate::services::{get_service, TestContext, TestInput};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::time::Duration;

/// Test runner configuration
pub struct TestRunnerConfig {
    /// Services to test
    pub services: Vec<String>,
    /// API key for authentication
    pub api_key: Option<String>,
    /// Azure region
    pub region: String,
    /// Cloud environment
    pub cloud: Cloud,
    /// Authentication method
    pub auth_method: AuthMethod,
    /// Request timeout
    pub timeout: Duration,
    /// Custom endpoint
    pub endpoint: Option<String>,
    /// Input file path
    pub input_file: Option<String>,
    /// Specific scenarios to run
    pub scenarios: Option<Vec<String>>,
    /// Show verbose output
    pub verbose: bool,
    /// Quiet mode (no progress indicators)
    pub quiet: bool,
}

impl TestRunnerConfig {
    /// Create from CLI args and config file
    pub fn from_config(
        config: &Config,
        services: Vec<String>,
        api_key: Option<String>,
        region: Option<String>,
        cloud: Option<Cloud>,
        auth_method: Option<AuthMethod>,
        timeout: Option<u64>,
        endpoint: Option<String>,
        input_file: Option<String>,
        scenarios: Option<Vec<String>>,
        verbose: bool,
        quiet: bool,
    ) -> Self {
        // Use provided values or fall back to config
        let api_key = api_key.or_else(|| {
            // Try to find an API key from any configured service
            for service_name in &services {
                if let Some(svc) = config.services.get(service_name) {
                    if svc.api_key.is_some() {
                        return svc.api_key.clone();
                    }
                }
            }
            None
        });

        let region = region.or_else(|| {
            // Try to find a region from any configured service
            for service_name in &services {
                if let Some(svc) = config.services.get(service_name) {
                    if svc.region.is_some() {
                        return svc.region.clone();
                    }
                }
            }
            None
        }).unwrap_or_else(|| "eastus".to_string());

        Self {
            services,
            api_key,
            region,
            cloud: cloud.unwrap_or(config.global.cloud),
            auth_method: auth_method.unwrap_or(config.auth.default_method),
            timeout: Duration::from_secs(timeout.unwrap_or(config.global.timeout_seconds)),
            endpoint,
            input_file: input_file.or(config.custom_inputs.audio_file.clone()),
            scenarios,
            verbose,
            quiet,
        }
    }
}

/// Test runner
pub struct TestRunner {
    config: TestRunnerConfig,
}

impl TestRunner {
    pub fn new(config: TestRunnerConfig) -> Self {
        Self { config }
    }

    /// Load input file if specified
    fn load_input(&self) -> Result<Option<TestInput>> {
        let path = match &self.config.input_file {
            Some(p) => p,
            None => return Ok(None),
        };

        let path = Path::new(path);
        if !path.exists() {
            return Err(AppError::FileNotFound(path.display().to_string()));
        }

        let data = std::fs::read(path)?;
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let content_type = match extension.as_str() {
            "wav" => "audio/wav",
            "mp3" => "audio/mpeg",
            "ogg" => "audio/ogg",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "pdf" => "application/pdf",
            "tiff" | "tif" => "image/tiff",
            _ => "application/octet-stream",
        };

        Ok(Some(TestInput {
            data,
            content_type: content_type.to_string(),
            file_name: path.file_name().map(|n| n.to_string_lossy().to_string()),
            text: None,
        }))
    }

    /// Get credentials based on auth method
    fn get_credentials(&self) -> Result<Credentials> {
        match self.config.auth_method {
            AuthMethod::Key | AuthMethod::Both => {
                let key = self
                    .config
                    .api_key
                    .clone()
                    .ok_or_else(|| AppError::Auth("API key not provided".to_string()))?;
                Ok(Credentials::ApiKey(key))
            }
            AuthMethod::Token => {
                // For token auth, we'd need to implement the full token flow
                // For now, return an error if no API key
                Err(AppError::Auth(
                    "Token auth requires Entra ID configuration".to_string(),
                ))
            }
        }
    }

    /// Run tests for all configured services
    pub async fn run(&self) -> Result<TestReport> {
        let credentials = self.get_credentials()?;
        let input = self.load_input()?;

        let mut all_results = Vec::new();

        // Create progress bar if not quiet
        let progress = if !self.config.quiet {
            let pb = ProgressBar::new(self.config.services.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("##-"),
            );
            Some(pb)
        } else {
            None
        };

        for service_name in &self.config.services {
            if let Some(pb) = &progress {
                pb.set_message(format!("Testing {}", service_name));
            }

            let service = match get_service(service_name) {
                Some(s) => s,
                None => {
                    if self.config.verbose {
                        eprintln!("Unknown service: {}", service_name);
                    }
                    continue;
                }
            };

            let context = TestContext::new(
                credentials.clone(),
                self.config.cloud,
                self.config.region.clone(),
                self.config.timeout,
            )?
            .with_endpoint(self.config.endpoint.clone())
            .with_input(input.clone())
            .with_verbose(self.config.verbose);

            let results = service
                .run_all_scenarios(&context, self.config.scenarios.as_deref())
                .await;

            all_results.push(results);

            if let Some(pb) = &progress {
                pb.inc(1);
            }
        }

        if let Some(pb) = progress {
            pb.finish_with_message("Complete");
        }

        Ok(TestReport::new(all_results))
    }
}

/// List available scenarios for a service
pub fn list_scenarios(service_name: Option<&str>) -> Vec<(String, Vec<crate::services::TestScenario>)> {
    use crate::services::get_all_services;

    let services = if let Some(name) = service_name {
        match get_service(name) {
            Some(s) => vec![s],
            None => vec![],
        }
    } else {
        get_all_services()
    };

    services
        .into_iter()
        .map(|s| (s.display_name().to_string(), s.list_scenarios()))
        .collect()
}

/// Format scenarios for display
pub fn format_scenarios(scenarios: &[(String, Vec<crate::services::TestScenario>)]) -> String {
    use console::style;

    let mut output = String::new();
    output.push_str("\nAvailable Test Scenarios\n");
    output.push_str("========================\n\n");

    for (service_name, service_scenarios) in scenarios {
        output.push_str(&format!("{}\n", style(service_name).bold()));
        output.push_str(&format!("{}\n", "-".repeat(service_name.len())));

        for scenario in service_scenarios {
            let input_marker = if scenario.requires_input {
                format!(
                    " [requires {}]",
                    scenario
                        .input_type
                        .map(|t| t.to_string())
                        .unwrap_or_default()
                )
            } else {
                String::new()
            };

            output.push_str(&format!(
                "  {} - {}{}\n",
                style(scenario.id).cyan(),
                scenario.description,
                style(input_marker).dim()
            ));
        }
        output.push('\n');
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_scenarios() {
        let scenarios = list_scenarios(None);
        assert!(!scenarios.is_empty());

        // Check that we have scenarios for each service
        let service_names: Vec<_> = scenarios.iter().map(|(name, _)| name.as_str()).collect();
        assert!(service_names.contains(&"Speech"));
        assert!(service_names.contains(&"Translator"));
    }

    #[test]
    fn test_list_scenarios_specific_service() {
        let scenarios = list_scenarios(Some("speech"));
        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].0, "Speech");
    }
}
