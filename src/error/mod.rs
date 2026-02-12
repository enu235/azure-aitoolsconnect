use thiserror::Error;

/// Maximum length for error messages exposed to users
const MAX_ERROR_LEN: usize = 200;

/// Sanitize error messages to prevent information leakage.
/// - Server errors (5xx) return a generic message to avoid exposing internal details
/// - Long messages are truncated to prevent sensitive data in stack traces
pub fn sanitize_error(body: &str, status: u16) -> String {
    if status >= 500 {
        return format!("Server error (HTTP {})", status);
    }
    if body.len() > MAX_ERROR_LEN {
        format!("{}... (truncated)", &body[..MAX_ERROR_LEN])
    } else {
        body.to_string()
    }
}

/// Exit codes for the CLI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    /// All tests passed
    Success = 0,
    /// Some tests failed
    TestFailure = 1,
    /// Authentication failure
    AuthFailure = 2,
    /// Network failure
    NetworkFailure = 3,
    /// Configuration error
    ConfigError = 4,
    /// Invalid input
    InvalidInput = 5,
}

impl From<ExitCode> for i32 {
    fn from(code: ExitCode) -> i32 {
        code as i32
    }
}

/// Main error type for the application
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Service error: {service} - {message}")]
    Service { service: String, message: String },

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("Test scenario '{scenario}' failed: {reason}")]
    TestFailed { scenario: String, reason: String },

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Device code authentication failed: {0}")]
    DeviceCodeAuthFailed(String),

    #[error("Managed identity not available: {0}")]
    ManagedIdentityNotAvailable(String),

    #[error("Invalid bearer token: {0}")]
    InvalidBearerToken(String),

    #[error("User authentication requires tenant ID")]
    MissingTenantId,
}

impl AppError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            AppError::Config(_) | AppError::TomlParse(_) => ExitCode::ConfigError,
            AppError::Auth(_)
            | AppError::DeviceCodeAuthFailed(_)
            | AppError::ManagedIdentityNotAvailable(_)
            | AppError::InvalidBearerToken(_)
            | AppError::MissingTenantId => ExitCode::AuthFailure,
            AppError::Network(_) | AppError::Http(_) | AppError::Timeout(_) => {
                ExitCode::NetworkFailure
            }
            AppError::InvalidInput(_)
            | AppError::FileNotFound(_)
            | AppError::UrlParse(_)
            | AppError::Io(_) => ExitCode::InvalidInput,
            AppError::Service { .. } | AppError::TestFailed { .. } | AppError::Json(_) => {
                ExitCode::TestFailure
            }
        }
    }

    /// Return actionable guidance to help the user fix the issue
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            AppError::MissingTenantId => Some(
                "Use --tenant YOUR_TENANT_ID or set AZURE_USER_TENANT_ID environment variable.\n  \
                 Find your tenant ID: Azure Portal > Microsoft Entra ID > Overview > Tenant ID"
            ),
            AppError::InvalidBearerToken(msg) if msg.contains("empty") => Some(
                "Provide a token with --bearer-token TOKEN or set AZURE_BEARER_TOKEN.\n  \
                 Get a token interactively: azure-aitoolsconnect login --tenant YOUR_TENANT_ID"
            ),
            AppError::InvalidBearerToken(_) => Some(
                "Ensure you are using a valid JWT token (they typically start with 'eyJ').\n  \
                 Get a fresh token: azure-aitoolsconnect login --tenant YOUR_TENANT_ID"
            ),
            AppError::DeviceCodeAuthFailed(msg) if msg.contains("timed out") || msg.contains("expired") => Some(
                "The device code expired before sign-in completed. Run the command again\n  \
                 and complete the sign-in within the time limit shown."
            ),
            AppError::DeviceCodeAuthFailed(msg) if msg.contains("declined") => Some(
                "Authorization was denied. Ensure your account has the 'Cognitive Services User'\n  \
                 RBAC role assigned on the target Azure AI resource."
            ),
            AppError::DeviceCodeAuthFailed(_) => Some(
                "Check your network connection and tenant ID. If the issue persists, verify\n  \
                 the tenant allows device code authentication in Entra ID settings."
            ),
            AppError::Auth(msg) if msg.contains("401") || msg.contains("403") || msg.contains("Unauthorized") => Some(
                "Authentication was rejected. For bearer token auth, ensure:\n  \
                 1. Use a custom subdomain endpoint (--endpoint https://YOUR-RESOURCE.cognitiveservices.azure.com)\n  \
                 2. The 'Cognitive Services User' RBAC role is assigned to your identity\n  \
                 3. Your token has not expired (get a fresh one with: azure-aitoolsconnect login)"
            ),
            AppError::Auth(msg) if msg.contains("API key") || msg.contains("api key") => Some(
                "Set an API key with --api-key YOUR_KEY or AZURE_AI_API_KEY environment variable.\n  \
                 Or use interactive login: azure-aitoolsconnect test --auth device-code --tenant YOUR_TENANT_ID"
            ),
            AppError::Config(msg) if msg.contains("API key") || msg.contains("not configured") => Some(
                "Set an API key with --api-key YOUR_KEY or AZURE_AI_API_KEY environment variable.\n  \
                 For interactive login: azure-aitoolsconnect test --auth device-code --tenant YOUR_TENANT_ID\n  \
                 To create a config file: azure-aitoolsconnect init"
            ),
            AppError::ManagedIdentityNotAvailable(_) => Some(
                "Managed identity is only available in Azure environments (VM, App Service, etc.).\n  \
                 For local development, use: azure-aitoolsconnect test --auth device-code --tenant YOUR_TENANT_ID"
            ),
            AppError::Network(msg) if msg.contains("dns") || msg.contains("resolve") || msg.contains("DNS") => Some(
                "DNS resolution failed. Check your network connection and proxy settings.\n  \
                 Run diagnostics: azure-aitoolsconnect diagnose --dns --region YOUR_REGION"
            ),
            AppError::Network(_) | AppError::Http(_) | AppError::Timeout(_) => Some(
                "Check your network connection. If behind a proxy or firewall, ensure Azure\n  \
                 endpoints are accessible. Run: azure-aitoolsconnect diagnose --region YOUR_REGION"
            ),
            AppError::FileNotFound(path) if path.contains("config") => Some(
                "Create a config file: azure-aitoolsconnect init\n  \
                 Or specify a path: azure-aitoolsconnect --config /path/to/config.toml"
            ),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
