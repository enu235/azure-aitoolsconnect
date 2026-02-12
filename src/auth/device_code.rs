use super::{AuthProvider, Credentials};
use crate::config::Cloud;
use crate::error::{AppError, Result};
use async_trait::async_trait;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use oauth2::basic::{BasicClient, BasicTokenResponse};
use oauth2::devicecode::StandardDeviceAuthorizationResponse;
use oauth2::{
    AuthUrl, ClientId, DeviceAuthorizationUrl, DeviceCodeErrorResponseType, RequestTokenError,
    Scope, TokenResponse, TokenUrl,
};
use std::time::Duration;
use tokio::time::sleep;

/// Azure CLI's well-known public client ID
const AZURE_CLI_CLIENT_ID: &str = "04b07795-8ddb-461a-bbee-02f9e1bf7b46";

/// Token result with metadata for display and caching
#[derive(Debug, Clone)]
pub struct TokenResult {
    pub access_token: String,
    pub expires_in_secs: u64,
    pub scope: String,
}

/// Device Code Flow authentication provider
/// Displays a code for the user to enter at a Microsoft login page
pub struct DeviceCodeAuth {
    tenant_id: String,
    client_id: String,
    scope: String,
    cloud: Cloud,
    quiet: bool,
}

impl DeviceCodeAuth {
    pub fn new(tenant_id: String, client_id: Option<String>, cloud: &Cloud) -> Result<Self> {
        let client_id = client_id.unwrap_or_else(|| AZURE_CLI_CLIENT_ID.to_string());

        let scope = match cloud {
            Cloud::Global => "https://cognitiveservices.azure.com/.default",
            Cloud::China => "https://cognitiveservices.azure.cn/.default",
        };

        Ok(Self {
            tenant_id,
            client_id,
            scope: scope.to_string(),
            cloud: *cloud,
            quiet: false,
        })
    }

    /// Set quiet mode (suppresses progress indicators)
    pub fn with_quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Authenticate and return token with metadata.
    /// This is the public API for the login command.
    pub async fn authenticate(&self) -> Result<TokenResult> {
        self.fetch_token().await
    }

    /// Fetch token using device code flow
    async fn fetch_token(&self) -> Result<TokenResult> {
        let login_endpoint = self.cloud.login_endpoint();

        // Build OAuth2 client
        let device_auth_url = DeviceAuthorizationUrl::new(format!(
            "{}/{}/oauth2/v2.0/devicecode",
            login_endpoint, self.tenant_id
        ))
        .map_err(|e| AppError::DeviceCodeAuthFailed(format!("Invalid device auth URL: {}", e)))?;

        let token_url = TokenUrl::new(format!(
            "{}/{}/oauth2/v2.0/token",
            login_endpoint, self.tenant_id
        ))
        .map_err(|e| AppError::DeviceCodeAuthFailed(format!("Invalid token URL: {}", e)))?;

        let auth_url = AuthUrl::new(format!(
            "{}/{}/oauth2/v2.0/authorize",
            login_endpoint, self.tenant_id
        ))
        .map_err(|e| AppError::DeviceCodeAuthFailed(format!("Invalid auth URL: {}", e)))?;

        let client = BasicClient::new(
            ClientId::new(self.client_id.clone()),
            None,
            auth_url,
            Some(token_url),
        )
        .set_device_authorization_url(device_auth_url);

        // Request device code
        let details: StandardDeviceAuthorizationResponse = client
            .exchange_device_code()
            .map_err(|e| {
                AppError::DeviceCodeAuthFailed(format!("Failed to initiate device code flow: {}", e))
            })?
            .add_scope(Scope::new(self.scope.clone()))
            .request_async(oauth2::reqwest::async_http_client)
            .await
            .map_err(|e| {
                AppError::DeviceCodeAuthFailed(format!("Device code request failed: {}", e))
            })?;

        // Display instructions to user
        self.display_instructions(&details);

        // Poll for token
        let token = self.poll_for_token(&client, &details).await?;

        let expires_in = token
            .expires_in()
            .map(|d| d.as_secs())
            .unwrap_or(3600);

        Ok(TokenResult {
            access_token: token.access_token().secret().clone(),
            expires_in_secs: expires_in,
            scope: self.scope.clone(),
        })
    }

    /// Display authentication instructions to the user
    fn display_instructions(&self, details: &StandardDeviceAuthorizationResponse) {
        let url = details.verification_uri().as_str();
        let code = details.user_code().secret();

        eprintln!();
        eprintln!("{}", style("======================================================================").cyan());
        eprintln!("  {} {}", style("[*]").cyan(), style("Azure Authentication Required").bold());
        eprintln!("{}", style("======================================================================").cyan());
        eprintln!();
        eprintln!("  {} Open this URL in your browser:", style("1.").bold());
        eprintln!("     {}", style(url).underlined());
        eprintln!();
        eprintln!("  {} Enter this code:", style("2.").bold());
        eprintln!("     {}", style(code).bold().yellow());
        eprintln!();
        eprintln!("{}", style("======================================================================").cyan());
        eprintln!();
    }

    /// Poll the token endpoint until the user completes authentication
    async fn poll_for_token(
        &self,
        client: &BasicClient,
        details: &StandardDeviceAuthorizationResponse,
    ) -> Result<BasicTokenResponse> {
        let interval = details.interval();
        let timeout_secs = details.expires_in().as_secs();
        let timeout_secs = if timeout_secs == 0 { 15 * 60 } else { timeout_secs };
        let timeout = Duration::from_secs(timeout_secs);
        let start = std::time::Instant::now();

        // Create countdown progress bar
        let pb = if !self.quiet {
            let pb = ProgressBar::new(timeout_secs);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("  {spinner:.cyan} Waiting for sign-in... [{bar:30.dim}] {msg}")
                    .unwrap()
                    .progress_chars("=>-"),
            );
            Some(pb)
        } else {
            None
        };

        loop {
            let elapsed = start.elapsed();
            if elapsed > timeout {
                if let Some(ref pb) = pb {
                    pb.finish_and_clear();
                }
                return Err(AppError::DeviceCodeAuthFailed(
                    "Authentication timed out. Please try again.".to_string(),
                ));
            }

            // Update countdown message
            if let Some(ref pb) = pb {
                let remaining = timeout_secs.saturating_sub(elapsed.as_secs());
                let mins = remaining / 60;
                let secs = remaining % 60;
                pb.set_position(elapsed.as_secs());
                pb.set_message(format!("{}:{:02} remaining", mins, secs));
            }

            sleep(interval).await;

            match client
                .exchange_device_access_token(details)
                .request_async(
                    oauth2::reqwest::async_http_client,
                    tokio::time::sleep,
                    None,
                )
                .await
            {
                Ok(token) => {
                    if let Some(ref pb) = pb {
                        pb.finish_and_clear();
                    }
                    eprintln!("  {} {}", style("[+]").green(), style("Authentication successful!").green().bold());
                    eprintln!();
                    return Ok(token);
                }
                Err(RequestTokenError::ServerResponse(err)) => {
                    match err.error() {
                        DeviceCodeErrorResponseType::AuthorizationPending => {
                            // Still waiting for user - continue polling
                            continue;
                        }
                        DeviceCodeErrorResponseType::SlowDown => {
                            // Server requested slower polling - add extra delay
                            sleep(interval).await;
                            continue;
                        }
                        DeviceCodeErrorResponseType::ExpiredToken => {
                            if let Some(ref pb) = pb {
                                pb.finish_and_clear();
                            }
                            return Err(AppError::DeviceCodeAuthFailed(
                                "Device code expired. Please try again.".to_string(),
                            ));
                        }
                        DeviceCodeErrorResponseType::AccessDenied => {
                            if let Some(ref pb) = pb {
                                pb.finish_and_clear();
                            }
                            return Err(AppError::DeviceCodeAuthFailed(
                                "User declined authorization".to_string(),
                            ));
                        }
                        _ => {
                            if let Some(ref pb) = pb {
                                pb.finish_and_clear();
                            }
                            return Err(AppError::DeviceCodeAuthFailed(format!(
                                "Server error: {:?}",
                                err
                            )));
                        }
                    }
                }
                Err(RequestTokenError::Request(e)) => {
                    if let Some(ref pb) = pb {
                        pb.finish_and_clear();
                    }
                    return Err(AppError::DeviceCodeAuthFailed(format!(
                        "Network error during token request: {}",
                        e
                    )));
                }
                Err(e) => {
                    if let Some(ref pb) = pb {
                        pb.finish_and_clear();
                    }
                    return Err(AppError::DeviceCodeAuthFailed(format!(
                        "Token request failed: {}",
                        e
                    )));
                }
            }
        }
    }
}

#[async_trait]
impl AuthProvider for DeviceCodeAuth {
    async fn get_credentials(&self) -> Result<Credentials> {
        let result = self.fetch_token().await?;
        Ok(Credentials::BearerToken(result.access_token))
    }

    fn method_name(&self) -> &'static str {
        "Device Code Flow"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_code_auth_creation() {
        let auth = DeviceCodeAuth::new(
            "tenant-id".to_string(),
            None,
            &Cloud::Global,
        );
        assert!(auth.is_ok());
        let auth = auth.unwrap();
        assert_eq!(auth.client_id, AZURE_CLI_CLIENT_ID);
    }

    #[test]
    fn test_device_code_auth_custom_client_id() {
        let custom_id = "custom-client-id".to_string();
        let auth = DeviceCodeAuth::new(
            "tenant-id".to_string(),
            Some(custom_id.clone()),
            &Cloud::Global,
        );
        assert!(auth.is_ok());
        let auth = auth.unwrap();
        assert_eq!(auth.client_id, custom_id);
    }

    #[test]
    fn test_china_cloud_scope() {
        let auth = DeviceCodeAuth::new(
            "tenant-id".to_string(),
            None,
            &Cloud::China,
        )
        .unwrap();
        assert!(auth.scope.contains("cognitiveservices.azure.cn"));
    }

    #[test]
    fn test_token_result_structure() {
        let result = TokenResult {
            access_token: "test-token".to_string(),
            expires_in_secs: 3600,
            scope: "https://cognitiveservices.azure.com/.default".to_string(),
        };
        assert_eq!(result.expires_in_secs, 3600);
        assert_eq!(result.access_token, "test-token");
    }

    #[test]
    fn test_quiet_mode() {
        let auth = DeviceCodeAuth::new(
            "tenant-id".to_string(),
            None,
            &Cloud::Global,
        )
        .unwrap()
        .with_quiet(true);
        assert!(auth.quiet);
    }
}
