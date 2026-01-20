use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Azure AI Services Connectivity Testing CLI Tool
#[derive(Parser, Debug)]
#[command(name = "azure-aitoolsconnect")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Path to configuration file
    #[arg(short, long, global = true, env = "AZURE_AITOOLSCONNECT_CONFIG")]
    pub config: Option<PathBuf>,

    /// Enable verbose output
    #[arg(short, long, global = true, default_value_t = false)]
    pub verbose: bool,

    /// Suppress progress indicators
    #[arg(short, long, global = true, default_value_t = false)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run connectivity tests against Azure AI Services
    Test(TestArgs),

    /// Run network diagnostics
    Diagnose(DiagnoseArgs),

    /// Initialize a new configuration file
    Init(InitArgs),

    /// Validate a configuration file
    Validate(ValidateArgs),

    /// List available test scenarios for a service
    ListScenarios(ListScenariosArgs),
}

#[derive(Args, Debug)]
pub struct TestArgs {
    /// Services to test (comma-separated, or 'all')
    #[arg(short, long, default_value = "all", value_delimiter = ',')]
    pub services: Vec<String>,

    /// API key for authentication
    #[arg(long, env = "AZURE_AI_API_KEY")]
    pub api_key: Option<String>,

    /// Azure region
    #[arg(short, long, env = "AZURE_REGION")]
    pub region: Option<String>,

    /// Authentication method
    #[arg(long, value_enum, default_value_t = AuthMethodArg::Key)]
    pub auth: AuthMethodArg,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = OutputFormatArg::Human)]
    pub output: OutputFormatArg,

    /// Cloud environment
    #[arg(long, value_enum, default_value_t = CloudArg::Global)]
    pub cloud: CloudArg,

    /// Custom input file for testing (audio, image, or document)
    #[arg(long)]
    pub input_file: Option<PathBuf>,

    /// Request timeout in seconds
    #[arg(long, default_value_t = 30)]
    pub timeout: u64,

    /// Test scenarios to run (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub scenarios: Option<Vec<String>>,

    /// Custom endpoint URL (overrides region-based endpoint)
    #[arg(long)]
    pub endpoint: Option<String>,

    /// Write output to file
    #[arg(long)]
    pub output_file: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct DiagnoseArgs {
    /// Run DNS diagnostics
    #[arg(long, default_value_t = false)]
    pub dns: bool,

    /// Run TLS diagnostics
    #[arg(long, default_value_t = false)]
    pub tls: bool,

    /// Run latency diagnostics
    #[arg(long, default_value_t = false)]
    pub latency: bool,

    /// Target endpoint for diagnostics
    #[arg(short, long)]
    pub endpoint: Option<String>,

    /// Azure region
    #[arg(short, long, env = "AZURE_REGION")]
    pub region: Option<String>,

    /// Cloud environment
    #[arg(long, value_enum, default_value_t = CloudArg::Global)]
    pub cloud: CloudArg,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = OutputFormatArg::Human)]
    pub output: OutputFormatArg,
}

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Output path for the configuration file
    #[arg(short, long, default_value = "./config.toml")]
    pub output: PathBuf,

    /// Overwrite existing file
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Path to configuration file to validate
    #[arg(short, long, default_value = "./config.toml")]
    pub config: PathBuf,
}

#[derive(Args, Debug)]
pub struct ListScenariosArgs {
    /// Service to list scenarios for
    #[arg(short, long)]
    pub service: Option<String>,
}

#[derive(ValueEnum, Clone, Debug, Default)]
pub enum AuthMethodArg {
    #[default]
    Key,
    Token,
    Both,
}

impl From<AuthMethodArg> for crate::config::AuthMethod {
    fn from(arg: AuthMethodArg) -> Self {
        match arg {
            AuthMethodArg::Key => crate::config::AuthMethod::Key,
            AuthMethodArg::Token => crate::config::AuthMethod::Token,
            AuthMethodArg::Both => crate::config::AuthMethod::Both,
        }
    }
}

#[derive(ValueEnum, Clone, Debug, Default)]
pub enum OutputFormatArg {
    #[default]
    Human,
    Json,
    Junit,
}

impl From<OutputFormatArg> for crate::config::OutputFormat {
    fn from(arg: OutputFormatArg) -> Self {
        match arg {
            OutputFormatArg::Human => crate::config::OutputFormat::Human,
            OutputFormatArg::Json => crate::config::OutputFormat::Json,
            OutputFormatArg::Junit => crate::config::OutputFormat::Junit,
        }
    }
}

#[derive(ValueEnum, Clone, Debug, Default)]
pub enum CloudArg {
    #[default]
    Global,
    China,
}

impl From<CloudArg> for crate::config::Cloud {
    fn from(arg: CloudArg) -> Self {
        match arg {
            CloudArg::Global => crate::config::Cloud::Global,
            CloudArg::China => crate::config::Cloud::China,
        }
    }
}

/// Parse services argument, handling "all" specially
pub fn parse_services(services: &[String]) -> Vec<String> {
    if services.len() == 1 && services[0].to_lowercase() == "all" {
        vec![
            "speech".to_string(),
            "translator".to_string(),
            "language".to_string(),
            "vision".to_string(),
            "document_intelligence".to_string(),
        ]
    } else {
        services
            .iter()
            .map(|s| s.to_lowercase().replace('-', "_"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_services_all() {
        let result = parse_services(&["all".to_string()]);
        assert_eq!(result.len(), 5);
        assert!(result.contains(&"speech".to_string()));
    }

    #[test]
    fn test_parse_services_specific() {
        let result = parse_services(&["speech".to_string(), "translator".to_string()]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_parse_services_normalize() {
        let result = parse_services(&["document-intelligence".to_string()]);
        assert_eq!(result[0], "document_intelligence");
    }
}
