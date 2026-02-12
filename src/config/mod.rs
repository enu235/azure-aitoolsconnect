use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::error::{AppError, Result};

/// Default request timeout in seconds
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum input file size in bytes (10MB)
pub const MAX_INPUT_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Cloud environment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cloud {
    #[default]
    Global,
    China,
}

impl Cloud {
    /// Get the Entra ID login endpoint for this cloud
    pub fn login_endpoint(&self) -> &'static str {
        match self {
            Cloud::Global => "https://login.microsoftonline.com",
            Cloud::China => "https://login.partner.microsoftonline.cn",
        }
    }

    /// Get the cognitive services token endpoint for this cloud
    pub fn cognitive_token_endpoint(&self, region: &str) -> String {
        match self {
            Cloud::Global => {
                format!(
                    "https://{}.api.cognitive.microsoft.com/sts/v1.0/issueToken",
                    region
                )
            }
            Cloud::China => {
                format!(
                    "https://{}.api.cognitive.azure.cn/sts/v1.0/issueToken",
                    region
                )
            }
        }
    }

    /// Get the cognitive services token endpoint, using a custom endpoint if provided
    pub fn cognitive_token_endpoint_for(
        &self,
        region: &str,
        custom_endpoint: Option<&str>,
    ) -> String {
        if let Some(endpoint) = custom_endpoint {
            format!("{}/sts/v1.0/issueToken", endpoint.trim_end_matches('/'))
        } else {
            self.cognitive_token_endpoint(region)
        }
    }

    /// Get the default cognitive services scope for Entra ID auth
    pub fn cognitive_scope(&self) -> &'static str {
        match self {
            Cloud::Global => "https://cognitiveservices.azure.com/.default",
            Cloud::China => "https://cognitiveservices.azure.cn/.default",
        }
    }
}

impl std::fmt::Display for Cloud {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cloud::Global => write!(f, "global"),
            Cloud::China => write!(f, "china"),
        }
    }
}

impl std::str::FromStr for Cloud {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "global" | "azure" | "public" => Ok(Cloud::Global),
            "china" | "mooncake" | "cn" => Ok(Cloud::China),
            _ => Err(AppError::Config(format!("Unknown cloud: {}", s))),
        }
    }
}

/// Output format for test results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
    Junit,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Human => write!(f, "human"),
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Junit => write!(f, "junit"),
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "human" | "text" | "console" => Ok(OutputFormat::Human),
            "json" => Ok(OutputFormat::Json),
            "junit" | "xml" => Ok(OutputFormat::Junit),
            _ => Err(AppError::Config(format!("Unknown output format: {}", s))),
        }
    }
}

/// Authentication method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    #[default]
    Key,
    Token,
    Both,
    #[serde(rename = "device_code")]
    DeviceCode,
    #[serde(rename = "managed_identity")]
    ManagedIdentity,
    #[serde(rename = "manual_token")]
    ManualToken,
}

impl std::fmt::Display for AuthMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthMethod::Key => write!(f, "key"),
            AuthMethod::Token => write!(f, "token"),
            AuthMethod::Both => write!(f, "both"),
            AuthMethod::DeviceCode => write!(f, "device-code"),
            AuthMethod::ManagedIdentity => write!(f, "managed-identity"),
            AuthMethod::ManualToken => write!(f, "manual-token"),
        }
    }
}

impl std::str::FromStr for AuthMethod {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "key" | "apikey" | "api-key" => Ok(AuthMethod::Key),
            "token" | "entra" | "aad" | "bearer" => Ok(AuthMethod::Token),
            "both" | "all" => Ok(AuthMethod::Both),
            "device-code" | "device_code" | "devicecode" => Ok(AuthMethod::DeviceCode),
            "managed-identity" | "managed_identity" | "managedidentity" | "mi" => {
                Ok(AuthMethod::ManagedIdentity)
            }
            "manual-token" | "manual_token" | "manualtoken" | "manual" => {
                Ok(AuthMethod::ManualToken)
            }
            _ => Err(AppError::Config(format!("Unknown auth method: {}", s))),
        }
    }
}

/// Global configuration settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalConfig {
    #[serde(default)]
    pub cloud: Cloud,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub output_format: OutputFormat,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

/// Entra ID (Azure AD) authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntraConfig {
    pub tenant_id: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
}

/// User authentication configuration (device code, managed identity, manual token)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserAuthConfig {
    /// Tenant ID for device code flow
    pub tenant_id: Option<String>,
    /// Custom public client ID (defaults to Azure CLI client ID if not specified)
    pub client_id: Option<String>,
    /// Client ID for user-assigned managed identity
    pub managed_identity_client_id: Option<String>,
    /// Bearer token for manual token authentication
    pub bearer_token: Option<String>,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub default_method: AuthMethod,
    #[serde(default)]
    pub entra: EntraConfig,
    #[serde(default)]
    pub user: UserAuthConfig,
}

/// Service-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub test_scenarios: Vec<String>,
}

fn default_enabled() -> bool {
    true
}

/// Custom input files configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CustomInputs {
    pub audio_file: Option<String>,
    pub document_file: Option<String>,
    pub image_file: Option<String>,
    pub text: Option<String>,
}

/// Complete application configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub global: GlobalConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub services: HashMap<String, ServiceConfig>,
    #[serde(default)]
    pub custom_inputs: CustomInputs,
}

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::FileNotFound(path.display().to_string())
            } else {
                AppError::Io(e)
            }
        })?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Create a default configuration
    pub fn default_config() -> Self {
        let mut services = HashMap::new();

        services.insert(
            "speech".to_string(),
            ServiceConfig {
                enabled: true,
                region: Some("eastus".to_string()),
                api_key: None,
                endpoint: None,
                test_scenarios: vec![
                    "endpoint_check".to_string(),
                    "voices_list".to_string(),
                    "token_exchange".to_string(),
                    "tts".to_string(),
                ],
            },
        );

        services.insert(
            "translator".to_string(),
            ServiceConfig {
                enabled: true,
                region: Some("global".to_string()),
                api_key: None,
                endpoint: None,
                test_scenarios: vec!["languages".to_string(), "detect".to_string()],
            },
        );

        services.insert(
            "language".to_string(),
            ServiceConfig {
                enabled: true,
                region: Some("eastus".to_string()),
                api_key: None,
                endpoint: None,
                test_scenarios: vec!["sentiment".to_string(), "language_detection".to_string()],
            },
        );

        services.insert(
            "vision".to_string(),
            ServiceConfig {
                enabled: true,
                region: Some("eastus".to_string()),
                api_key: None,
                endpoint: None,
                test_scenarios: vec!["analyze_image".to_string()],
            },
        );

        services.insert(
            "document_intelligence".to_string(),
            ServiceConfig {
                enabled: true,
                region: Some("eastus".to_string()),
                api_key: None,
                endpoint: None,
                test_scenarios: vec!["layout".to_string()],
            },
        );

        Config {
            global: GlobalConfig {
                cloud: Cloud::Global,
                timeout_seconds: DEFAULT_TIMEOUT_SECS,
                output_format: OutputFormat::Human,
            },
            auth: AuthConfig {
                default_method: AuthMethod::Key,
                entra: EntraConfig::default(),
                user: UserAuthConfig::default(),
            },
            services,
            custom_inputs: CustomInputs::default(),
        }
    }

    /// Serialize configuration to TOML string
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| AppError::Config(e.to_string()))
    }

    /// Get service configuration by name
    pub fn get_service(&self, name: &str) -> Option<&ServiceConfig> {
        self.services.get(name)
    }

    /// Get mutable service configuration by name
    pub fn get_service_mut(&mut self, name: &str) -> Option<&mut ServiceConfig> {
        self.services.get_mut(name)
    }

    /// Apply environment variable overrides
    pub fn apply_env_overrides(&mut self) {
        // Global API key
        if let Ok(key) = std::env::var("AZURE_AI_API_KEY") {
            for service in self.services.values_mut() {
                if service.api_key.is_none() {
                    service.api_key = Some(key.clone());
                }
            }
        }

        // Global endpoint (custom subdomain for multi-service resources)
        if let Ok(endpoint) = std::env::var("AZURE_AI_ENDPOINT") {
            for service in self.services.values_mut() {
                if service.endpoint.is_none() {
                    service.endpoint = Some(endpoint.clone());
                }
            }
        }

        // Service-specific API keys and endpoints
        for (name, service) in self.services.iter_mut() {
            let env_name = format!("AZURE_{}_API_KEY", name.to_uppercase().replace('-', "_"));
            if let Ok(key) = std::env::var(&env_name) {
                service.api_key = Some(key);
            }

            let env_region = format!("AZURE_{}_REGION", name.to_uppercase().replace('-', "_"));
            if let Ok(region) = std::env::var(&env_region) {
                service.region = Some(region);
            }

            let env_endpoint = format!("AZURE_{}_ENDPOINT", name.to_uppercase().replace('-', "_"));
            if let Ok(endpoint) = std::env::var(&env_endpoint) {
                service.endpoint = Some(endpoint);
            }
        }

        // Entra ID settings (service principal)
        if let Ok(tenant) = std::env::var("AZURE_TENANT_ID") {
            self.auth.entra.tenant_id = Some(tenant);
        }
        if let Ok(client_id) = std::env::var("AZURE_CLIENT_ID") {
            self.auth.entra.client_id = Some(client_id);
        }
        if let Ok(client_secret) = std::env::var("AZURE_CLIENT_SECRET") {
            self.auth.entra.client_secret = Some(client_secret);
        }

        // User authentication settings
        if let Ok(tenant) = std::env::var("AZURE_USER_TENANT_ID") {
            self.auth.user.tenant_id = Some(tenant);
        }
        if let Ok(token) = std::env::var("AZURE_BEARER_TOKEN") {
            self.auth.user.bearer_token = Some(token);
        }
        if let Ok(client_id) = std::env::var("AZURE_MI_CLIENT_ID") {
            self.auth.user.managed_identity_client_id = Some(client_id);
        }

        // Cloud setting
        if let Ok(cloud) = std::env::var("AZURE_CLOUD") {
            if let Ok(c) = cloud.parse() {
                self.global.cloud = c;
            }
        }

        // Global region
        if let Ok(region) = std::env::var("AZURE_REGION") {
            for service in self.services.values_mut() {
                if service.region.is_none() {
                    service.region = Some(region.clone());
                }
            }
        }
    }
}

/// Validate configuration
pub fn validate_config(config: &Config) -> Result<Vec<String>> {
    let mut warnings = Vec::new();

    // Check for enabled services without API keys (only relevant for key-based auth)
    if matches!(
        config.auth.default_method,
        AuthMethod::Key | AuthMethod::Both
    ) {
        for (name, service) in &config.services {
            if service.enabled && service.api_key.is_none() {
                warnings.push(format!(
                    "Service '{}' is enabled but has no API key configured",
                    name
                ));
            }
        }
    }

    // Check Entra config if token auth is selected
    if matches!(
        config.auth.default_method,
        AuthMethod::Token | AuthMethod::Both
    ) {
        if config.auth.entra.tenant_id.is_none() {
            warnings.push("Token auth selected but tenant_id is not configured".to_string());
        }
        if config.auth.entra.client_id.is_none() {
            warnings.push("Token auth selected but client_id is not configured".to_string());
        }
        if config.auth.entra.client_secret.is_none() {
            warnings.push("Token auth selected but client_secret is not configured".to_string());
        }
    }

    // Check device-code config
    if config.auth.default_method == AuthMethod::DeviceCode && config.auth.user.tenant_id.is_none()
    {
        warnings.push(
            "Device code auth selected but tenant_id is not configured in [auth.user]. \
             Use --tenant on the command line or set AZURE_USER_TENANT_ID."
                .to_string(),
        );
    }

    // Check manual-token config
    if config.auth.default_method == AuthMethod::ManualToken
        && config.auth.user.bearer_token.is_none()
    {
        warnings.push(
            "Manual token auth selected but no bearer_token is configured. \
             Use --bearer-token or set AZURE_BEARER_TOKEN."
                .to_string(),
        );
    }

    // Check managed-identity config (warn if outside Azure)
    if config.auth.default_method == AuthMethod::ManagedIdentity
        && std::env::var("IDENTITY_ENDPOINT").is_err()
        && std::env::var("MSI_ENDPOINT").is_err()
    {
        warnings.push(
            "Managed identity auth selected but no identity endpoint detected. \
             This will work on Azure VMs via IMDS but will fail on non-Azure machines."
                .to_string(),
        );
    }

    // Check bearer token auth without custom endpoint
    if matches!(
        config.auth.default_method,
        AuthMethod::DeviceCode | AuthMethod::ManualToken | AuthMethod::ManagedIdentity
    ) {
        let has_custom_endpoint = config.services.values().any(|s| s.endpoint.is_some());
        if !has_custom_endpoint {
            warnings.push(
                "Bearer token auth typically requires a custom subdomain endpoint. \
                 Use --endpoint https://YOUR-RESOURCE.cognitiveservices.azure.com"
                    .to_string(),
            );
        }
    }

    // Check for services with no region and no endpoint
    for (name, service) in &config.services {
        if service.enabled && service.region.is_none() && service.endpoint.is_none() {
            warnings.push(format!(
                "Service '{}' has no region or endpoint configured",
                name
            ));
        }
    }

    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cloud_endpoints() {
        assert_eq!(
            Cloud::Global.login_endpoint(),
            "https://login.microsoftonline.com"
        );
        assert_eq!(
            Cloud::China.login_endpoint(),
            "https://login.partner.microsoftonline.cn"
        );
    }

    #[test]
    fn test_cloud_parse() {
        assert_eq!("global".parse::<Cloud>().unwrap(), Cloud::Global);
        assert_eq!("china".parse::<Cloud>().unwrap(), Cloud::China);
        assert_eq!("mooncake".parse::<Cloud>().unwrap(), Cloud::China);
    }

    #[test]
    fn test_default_config() {
        let config = Config::default_config();
        assert_eq!(config.global.cloud, Cloud::Global);
        assert_eq!(config.global.timeout_seconds, DEFAULT_TIMEOUT_SECS);
        assert!(config.services.contains_key("speech"));
    }

    #[test]
    fn test_token_endpoint_custom_domain() {
        let ep = Cloud::Global.cognitive_token_endpoint_for(
            "eastus",
            Some("https://myservice.cognitiveservices.azure.com"),
        );
        assert_eq!(
            ep,
            "https://myservice.cognitiveservices.azure.com/sts/v1.0/issueToken"
        );
    }

    #[test]
    fn test_token_endpoint_regional_fallback() {
        let ep = Cloud::Global.cognitive_token_endpoint_for("eastus", None);
        assert_eq!(
            ep,
            "https://eastus.api.cognitive.microsoft.com/sts/v1.0/issueToken"
        );
    }

    #[test]
    fn test_token_endpoint_china_fallback() {
        let ep = Cloud::China.cognitive_token_endpoint_for("chinaeast2", None);
        assert_eq!(
            ep,
            "https://chinaeast2.api.cognitive.azure.cn/sts/v1.0/issueToken"
        );
    }

    #[test]
    fn test_token_endpoint_trailing_slash() {
        let ep = Cloud::Global.cognitive_token_endpoint_for(
            "eastus",
            Some("https://myservice.cognitiveservices.azure.com/"),
        );
        assert_eq!(
            ep,
            "https://myservice.cognitiveservices.azure.com/sts/v1.0/issueToken"
        );
    }
}
