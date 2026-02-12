use super::{AuthProvider, Credentials};
use crate::config::Cloud;
use crate::error::{AppError, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::env;
use std::time::Duration;

/// Managed Identity endpoint types
enum ManagedIdentityEndpoint {
    /// App Service / Container Apps (has identity endpoint and header)
    AppService { endpoint: String, header: String },
    /// Virtual Machine (uses IMDS)
    VirtualMachine,
}

/// Response from managed identity token endpoint
#[derive(Deserialize)]
struct ManagedIdentityResponse {
    access_token: String,
    #[allow(dead_code)]
    expires_in: String,
}

/// Managed Identity authentication provider
/// Automatically obtains tokens from Azure managed identity service
pub struct ManagedIdentityAuth {
    client: Client,
    endpoint: ManagedIdentityEndpoint,
    resource: String,
    user_assigned_client_id: Option<String>,
}

impl ManagedIdentityAuth {
    pub fn new(cloud: &Cloud, user_assigned_client_id: Option<String>) -> Result<Self> {
        // Detect environment
        let endpoint = Self::detect_endpoint()?;

        let resource = match cloud {
            Cloud::Global => "https://cognitiveservices.azure.com",
            Cloud::China => "https://cognitiveservices.azure.cn",
        };

        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| {
                AppError::ManagedIdentityNotAvailable(format!(
                    "Failed to create HTTP client: {}",
                    e
                ))
            })?;

        Ok(Self {
            client,
            endpoint,
            resource: resource.to_string(),
            user_assigned_client_id,
        })
    }

    /// Detect which managed identity endpoint to use based on environment variables
    fn detect_endpoint() -> Result<ManagedIdentityEndpoint> {
        // Check for App Service / Container Apps identity
        if let (Ok(endpoint), Ok(header)) =
            (env::var("IDENTITY_ENDPOINT"), env::var("IDENTITY_HEADER"))
        {
            return Ok(ManagedIdentityEndpoint::AppService { endpoint, header });
        }

        // Check for legacy MSI endpoint (older VM setup)
        if env::var("MSI_ENDPOINT").is_ok() {
            return Ok(ManagedIdentityEndpoint::VirtualMachine);
        }

        // Default to IMDS (VM)
        // Note: This will fail gracefully if not running on Azure VM
        Ok(ManagedIdentityEndpoint::VirtualMachine)
    }

    /// Fetch token from the appropriate managed identity endpoint
    async fn fetch_token(&self) -> Result<String> {
        match &self.endpoint {
            ManagedIdentityEndpoint::AppService { endpoint, header } => {
                self.fetch_app_service_token(endpoint, header).await
            }
            ManagedIdentityEndpoint::VirtualMachine => self.fetch_vm_token().await,
        }
    }

    /// Fetch token from App Service managed identity endpoint
    async fn fetch_app_service_token(&self, endpoint: &str, secret: &str) -> Result<String> {
        let mut url = format!(
            "{}?api-version=2019-08-01&resource={}",
            endpoint, self.resource
        );

        // Add client_id if using user-assigned identity
        if let Some(ref client_id) = self.user_assigned_client_id {
            url.push_str(&format!("&client_id={}", client_id));
        }

        let response = self
            .client
            .get(&url)
            .header("X-IDENTITY-HEADER", secret)
            .send()
            .await
            .map_err(|e| {
                AppError::ManagedIdentityNotAvailable(format!(
                    "Failed to reach App Service identity endpoint: {}",
                    e
                ))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AppError::ManagedIdentityNotAvailable(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        let mi_response: ManagedIdentityResponse = response.json().await.map_err(|e| {
            AppError::ManagedIdentityNotAvailable(format!("Failed to parse response: {}", e))
        })?;

        Ok(mi_response.access_token)
    }

    /// Fetch token from VM IMDS endpoint
    async fn fetch_vm_token(&self) -> Result<String> {
        let mut url = format!(
            "http://169.254.169.254/metadata/identity/oauth2/token\
             ?api-version=2018-02-01&resource={}",
            self.resource
        );

        // Add client_id if using user-assigned identity
        if let Some(ref client_id) = self.user_assigned_client_id {
            url.push_str(&format!("&client_id={}", client_id));
        }

        let response = self
            .client
            .get(&url)
            .header("Metadata", "true")
            .send()
            .await
            .map_err(|e| {
                AppError::ManagedIdentityNotAvailable(format!(
                    "Could not reach IMDS endpoint (169.254.169.254). \
                     This may not be an Azure VM, or managed identity is not enabled: {}",
                    e
                ))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AppError::ManagedIdentityNotAvailable(format!(
                "HTTP {}: {}. Ensure managed identity is enabled on this VM.",
                status, body
            )));
        }

        let mi_response: ManagedIdentityResponse = response.json().await.map_err(|e| {
            AppError::ManagedIdentityNotAvailable(format!("Failed to parse response: {}", e))
        })?;

        Ok(mi_response.access_token)
    }
}

#[async_trait]
impl AuthProvider for ManagedIdentityAuth {
    async fn get_credentials(&self) -> Result<Credentials> {
        let token = self.fetch_token().await?;
        Ok(Credentials::BearerToken(token))
    }

    fn method_name(&self) -> &'static str {
        "Managed Identity"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_endpoint_defaults_to_vm() {
        // Clear environment variables
        env::remove_var("IDENTITY_ENDPOINT");
        env::remove_var("IDENTITY_HEADER");
        env::remove_var("MSI_ENDPOINT");

        let result = ManagedIdentityAuth::detect_endpoint();
        assert!(result.is_ok());
        // Should default to VM endpoint
    }

    #[test]
    fn test_detect_app_service_endpoint() {
        env::set_var("IDENTITY_ENDPOINT", "http://localhost:8081");
        env::set_var("IDENTITY_HEADER", "test-header");

        let result = ManagedIdentityAuth::detect_endpoint();
        assert!(result.is_ok());

        // Clean up
        env::remove_var("IDENTITY_ENDPOINT");
        env::remove_var("IDENTITY_HEADER");
    }
}
