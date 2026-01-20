//! Azure AI Services Connectivity Testing Tool
//!
//! A CLI tool to help verify connectivity from clients to Azure AI Services
//! in complex network configurations.
//!
//! # Supported Services
//!
//! - **Speech** - speech-to-text, text-to-speech, translation
//! - **Translator** - 100+ language translation
//! - **Language** - sentiment analysis, NER, key phrase extraction
//! - **Document Intelligence** - document processing/OCR
//! - **Vision** - image analysis, OCR, object detection
//!
//! # Example Usage
//!
//! ```bash
//! # Test all services with API key
//! azure-aitoolsconnect test --services all --api-key $KEY --region eastus
//!
//! # Test specific services
//! azure-aitoolsconnect test --services speech,translator --region westus2
//!
//! # Run network diagnostics
//! azure-aitoolsconnect diagnose --dns --tls --latency --region eastus
//! ```

pub mod auth;
pub mod cli;
pub mod config;
pub mod error;
pub mod network;
pub mod output;
pub mod services;
pub mod testing;

pub use cli::{Cli, Commands};
pub use config::{AuthMethod, Cloud, Config, OutputFormat};
pub use error::{AppError, ExitCode, Result};
pub use output::{get_formatter, TestReport};
pub use services::{get_all_services, get_service, AzureService, TestResult};
pub use testing::{TestRunner, TestRunnerConfig};
