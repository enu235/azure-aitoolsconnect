use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

const MAIN_EXAMPLES: &str = "\
Quick start:
  azure-aitoolsconnect test --api-key YOUR_KEY --region eastus
  azure-aitoolsconnect login --tenant YOUR_TENANT_ID
  azure-aitoolsconnect diagnose --region eastus";

const TEST_EXAMPLES: &str = "\
EXAMPLES:
  # Test all services with an API key
  azure-aitoolsconnect test --api-key YOUR_KEY --region eastus

  # Test speech service with device code (interactive login)
  azure-aitoolsconnect test -s speech --auth device-code --tenant YOUR_TENANT_ID \\
    --endpoint https://your-resource.cognitiveservices.azure.com

  # Test with a manually obtained bearer token
  azure-aitoolsconnect test -s language --auth manual-token \\
    --bearer-token eyJ... --endpoint https://your-resource.cognitiveservices.azure.com

  # Test and display the bearer token for use in curl/Postman
  azure-aitoolsconnect test -s speech --auth device-code --tenant YOUR_TENANT_ID --show-token

  # Run specific test scenarios only
  azure-aitoolsconnect test -s speech --scenarios voices_list,tts --api-key KEY -r eastus

  # Output as JSON for scripting
  azure-aitoolsconnect test -s translator --api-key KEY -r eastus -o json

  # Output as JUnit XML for CI/CD
  azure-aitoolsconnect test --api-key KEY -o junit --output-file results.xml --quiet";

const LOGIN_EXAMPLES: &str = "\
EXAMPLES:
  # Interactive browser login (default, works with Conditional Access)
  azure-aitoolsconnect login --tenant YOUR_TENANT_ID

  # Get token and save for subsequent test commands
  azure-aitoolsconnect login --tenant YOUR_TENANT_ID --save

  # Device code flow (for headless/SSH environments)
  azure-aitoolsconnect login --auth device-code --tenant YOUR_TENANT_ID

  # Get token as JSON (for scripting)
  azure-aitoolsconnect login --tenant YOUR_TENANT_ID -o json

  # Use managed identity (on Azure VM/App Service)
  azure-aitoolsconnect login --auth managed-identity

  # Clear cached tokens
  azure-aitoolsconnect login --clear-cache";

const DIAGNOSE_EXAMPLES: &str = "\
EXAMPLES:
  # Full diagnostics for a region
  azure-aitoolsconnect diagnose --region eastus

  # DNS resolution check only
  azure-aitoolsconnect diagnose --dns --region eastus

  # Check a custom endpoint
  azure-aitoolsconnect diagnose -e your-resource.cognitiveservices.azure.com -r eastus";

/// Azure AI Services Connectivity Testing CLI Tool
///
/// Test connectivity from clients to Azure AI Services in complex network
/// configurations. Supports Speech, Translator, Language, Vision, and
/// Document Intelligence services.
#[derive(Parser, Debug)]
#[command(name = "azure-aitoolsconnect")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
#[command(after_help = MAIN_EXAMPLES)]
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

    /// Authenticate and obtain a bearer token
    Login(LoginArgs),

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
#[command(after_help = TEST_EXAMPLES)]
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

    /// Tenant ID (required for device-code auth method)
    #[arg(long, env = "AZURE_USER_TENANT_ID")]
    pub tenant: Option<String>,

    /// Bearer token (for manual-token auth method)
    #[arg(long, env = "AZURE_BEARER_TOKEN")]
    pub bearer_token: Option<String>,

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

    /// Display the bearer token after authentication (for use in curl/Postman)
    #[arg(long, default_value_t = false)]
    pub show_token: bool,

    /// Skip reading cached tokens from disk
    #[arg(long, default_value_t = false)]
    pub no_cache: bool,
}

#[derive(Args, Debug)]
#[command(after_help = LOGIN_EXAMPLES)]
pub struct LoginArgs {
    /// Tenant ID for authentication
    #[arg(long, env = "AZURE_USER_TENANT_ID")]
    pub tenant: Option<String>,

    /// Authentication method
    #[arg(long, value_enum, default_value_t = LoginAuthMethodArg::DeviceCode)]
    pub auth: LoginAuthMethodArg,

    /// Cloud environment
    #[arg(long, value_enum, default_value_t = CloudArg::Global)]
    pub cloud: CloudArg,

    /// Custom public client ID (advanced)
    #[arg(long)]
    pub client_id: Option<String>,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = OutputFormatArg::Human)]
    pub output: OutputFormatArg,

    /// Save token to cache for subsequent test commands
    #[arg(long, default_value_t = false)]
    pub save: bool,

    /// Clear cached tokens and exit
    #[arg(long, default_value_t = false)]
    pub clear_cache: bool,
}

#[derive(Args, Debug)]
#[command(after_help = DIAGNOSE_EXAMPLES)]
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

    /// Interactive setup wizard
    #[arg(short, long, default_value_t = false)]
    pub interactive: bool,
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
    #[value(name = "device-code")]
    DeviceCode,
    #[value(name = "managed-identity")]
    ManagedIdentity,
    #[value(name = "manual-token")]
    ManualToken,
    #[value(name = "interactive")]
    Interactive,
}

impl From<AuthMethodArg> for crate::config::AuthMethod {
    fn from(arg: AuthMethodArg) -> Self {
        match arg {
            AuthMethodArg::Key => crate::config::AuthMethod::Key,
            AuthMethodArg::Token => crate::config::AuthMethod::Token,
            AuthMethodArg::Both => crate::config::AuthMethod::Both,
            AuthMethodArg::DeviceCode => crate::config::AuthMethod::DeviceCode,
            AuthMethodArg::ManagedIdentity => crate::config::AuthMethod::ManagedIdentity,
            AuthMethodArg::ManualToken => crate::config::AuthMethod::ManualToken,
            AuthMethodArg::Interactive => crate::config::AuthMethod::Interactive,
        }
    }
}

/// Authentication methods available for the login command
#[derive(ValueEnum, Clone, Debug, Default)]
pub enum LoginAuthMethodArg {
    #[default]
    #[value(name = "interactive")]
    Interactive,
    #[value(name = "device-code")]
    DeviceCode,
    #[value(name = "managed-identity")]
    ManagedIdentity,
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
