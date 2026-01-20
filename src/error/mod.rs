use thiserror::Error;

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
}

impl AppError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            AppError::Config(_) | AppError::TomlParse(_) => ExitCode::ConfigError,
            AppError::Auth(_) => ExitCode::AuthFailure,
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
}

pub type Result<T> = std::result::Result<T, AppError>;
