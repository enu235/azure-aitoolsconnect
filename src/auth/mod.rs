use crate::config::{AuthMethod, Cloud, EntraConfig};
use crate::error::{AppError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Token response from Entra ID
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    #[allow(dead_code)]
    token_type: String,
}

/// Cached token with expiry
#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    expires_at: Instant,
}

impl CachedToken {
    fn is_expired(&self) -> bool {
        // Consider expired 60 seconds before actual expiry
        Instant::now() + Duration::from_secs(60) >= self.expires_at
    }
}

/// Authentication credentials
#[derive(Debug, Clone)]
pub enum Credentials {
    ApiKey(String),
    BearerToken(String),
}

impl Credentials {
    /// Apply credentials to a request builder
    pub fn apply_to_request(
        &self,
        request: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        match self {
            Credentials::ApiKey(key) => {
                request.header("Ocp-Apim-Subscription-Key", key)
            }
            Credentials::BearerToken(token) => {
                request.header("Authorization", format!("Bearer {}", token))
            }
        }
    }
}

/// Authentication provider trait
#[async_trait::async_trait]
pub trait AuthProvider: Send + Sync {
    /// Get credentials for making API calls
    async fn get_credentials(&self) -> Result<Credentials>;

    /// Get the authentication method name
    fn method_name(&self) -> &'static str;
}

/// API Key authentication provider
#[derive(Debug, Clone)]
pub struct ApiKeyAuth {
    api_key: String,
}

impl ApiKeyAuth {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[async_trait::async_trait]
impl AuthProvider for ApiKeyAuth {
    async fn get_credentials(&self) -> Result<Credentials> {
        Ok(Credentials::ApiKey(self.api_key.clone()))
    }

    fn method_name(&self) -> &'static str {
        "API Key"
    }
}

/// Entra ID (Azure AD) token authentication provider
pub struct EntraTokenAuth {
    client: Client,
    cloud: Cloud,
    tenant_id: String,
    client_id: String,
    client_secret: String,
    scope: String,
    cached_token: Arc<RwLock<Option<CachedToken>>>,
}

impl EntraTokenAuth {
    pub fn new(config: &EntraConfig, cloud: Cloud) -> Result<Self> {
        let tenant_id = config
            .tenant_id
            .clone()
            .ok_or_else(|| AppError::Auth("Missing tenant_id for Entra auth".to_string()))?;
        let client_id = config
            .client_id
            .clone()
            .ok_or_else(|| AppError::Auth("Missing client_id for Entra auth".to_string()))?;
        let client_secret = config
            .client_secret
            .clone()
            .ok_or_else(|| AppError::Auth("Missing client_secret for Entra auth".to_string()))?;

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::Network(e.to_string()))?;

        Ok(Self {
            client,
            cloud,
            tenant_id,
            client_id,
            client_secret,
            scope: cloud.cognitive_scope().to_string(),
            cached_token: Arc::new(RwLock::new(None)),
        })
    }

    async fn fetch_token(&self) -> Result<CachedToken> {
        let token_url = format!(
            "{}/{}/oauth2/v2.0/token",
            self.cloud.login_endpoint(),
            self.tenant_id
        );

        let params = [
            ("grant_type", "client_credentials"),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
            ("scope", &self.scope),
        ];

        let response = self
            .client
            .post(&token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| AppError::Auth(format!("Failed to request token: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AppError::Auth(format!(
                "Token request failed ({}): {}",
                status, body
            )));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| AppError::Auth(format!("Failed to parse token response: {}", e)))?;

        let expires_at = Instant::now() + Duration::from_secs(token_response.expires_in);

        Ok(CachedToken {
            token: token_response.access_token,
            expires_at,
        })
    }
}

#[async_trait::async_trait]
impl AuthProvider for EntraTokenAuth {
    async fn get_credentials(&self) -> Result<Credentials> {
        // Check cache first
        {
            let cache = self.cached_token.read().await;
            if let Some(ref cached) = *cache {
                if !cached.is_expired() {
                    return Ok(Credentials::BearerToken(cached.token.clone()));
                }
            }
        }

        // Fetch new token
        let new_token = self.fetch_token().await?;
        let token = new_token.token.clone();

        // Update cache
        {
            let mut cache = self.cached_token.write().await;
            *cache = Some(new_token);
        }

        Ok(Credentials::BearerToken(token))
    }

    fn method_name(&self) -> &'static str {
        "Entra ID Token"
    }
}

/// Cognitive Services token exchange (API key -> short-lived token)
pub struct CognitiveTokenAuth {
    client: Client,
    api_key: String,
    token_endpoint: String,
    cached_token: Arc<RwLock<Option<CachedToken>>>,
}

impl CognitiveTokenAuth {
    pub fn new(api_key: String, region: &str, cloud: Cloud) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::Network(e.to_string()))?;

        let token_endpoint = cloud.cognitive_token_endpoint(region);

        Ok(Self {
            client,
            api_key,
            token_endpoint,
            cached_token: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn exchange_token(&self) -> Result<String> {
        // Check cache first
        {
            let cache = self.cached_token.read().await;
            if let Some(ref cached) = *cache {
                if !cached.is_expired() {
                    return Ok(cached.token.clone());
                }
            }
        }

        // Fetch new token
        let response = self
            .client
            .post(&self.token_endpoint)
            .header("Ocp-Apim-Subscription-Key", &self.api_key)
            .header("Content-Length", "0")
            .send()
            .await
            .map_err(|e| AppError::Auth(format!("Token exchange failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AppError::Auth(format!(
                "Token exchange failed ({}): {}",
                status, body
            )));
        }

        let token = response
            .text()
            .await
            .map_err(|e| AppError::Auth(format!("Failed to read token: {}", e)))?;

        // Cognitive tokens expire in 10 minutes
        let cached = CachedToken {
            token: token.clone(),
            expires_at: Instant::now() + Duration::from_secs(540), // 9 minutes
        };

        // Update cache
        {
            let mut cache = self.cached_token.write().await;
            *cache = Some(cached);
        }

        Ok(token)
    }
}

#[async_trait::async_trait]
impl AuthProvider for CognitiveTokenAuth {
    async fn get_credentials(&self) -> Result<Credentials> {
        let token = self.exchange_token().await?;
        Ok(Credentials::BearerToken(token))
    }

    fn method_name(&self) -> &'static str {
        "Cognitive Token"
    }
}

/// Authentication manager that supports multiple auth methods
pub struct AuthManager {
    api_key: Option<ApiKeyAuth>,
    entra: Option<EntraTokenAuth>,
    default_method: AuthMethod,
}

impl AuthManager {
    pub fn new(
        api_key: Option<String>,
        entra_config: Option<&EntraConfig>,
        cloud: Cloud,
        default_method: AuthMethod,
    ) -> Result<Self> {
        let api_key_auth = api_key.map(ApiKeyAuth::new);

        let entra_auth = if let Some(config) = entra_config {
            if config.tenant_id.is_some()
                && config.client_id.is_some()
                && config.client_secret.is_some()
            {
                Some(EntraTokenAuth::new(config, cloud)?)
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            api_key: api_key_auth,
            entra: entra_auth,
            default_method,
        })
    }

    /// Get the primary auth provider based on configuration
    pub fn get_provider(&self) -> Result<&dyn AuthProvider> {
        match self.default_method {
            AuthMethod::Key => self
                .api_key
                .as_ref()
                .map(|a| a as &dyn AuthProvider)
                .ok_or_else(|| AppError::Auth("API key not configured".to_string())),
            AuthMethod::Token => self
                .entra
                .as_ref()
                .map(|a| a as &dyn AuthProvider)
                .ok_or_else(|| AppError::Auth("Entra ID not configured".to_string())),
            AuthMethod::Both => {
                // Prefer API key for simplicity
                if self.api_key.is_some() {
                    Ok(self.api_key.as_ref().unwrap() as &dyn AuthProvider)
                } else if self.entra.is_some() {
                    Ok(self.entra.as_ref().unwrap() as &dyn AuthProvider)
                } else {
                    Err(AppError::Auth("No authentication configured".to_string()))
                }
            }
        }
    }

    /// Get all configured auth providers
    pub fn get_all_providers(&self) -> Vec<&dyn AuthProvider> {
        let mut providers: Vec<&dyn AuthProvider> = Vec::new();
        if let Some(ref api_key) = self.api_key {
            providers.push(api_key);
        }
        if let Some(ref entra) = self.entra {
            providers.push(entra);
        }
        providers
    }

    /// Check if API key auth is available
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// Check if Entra auth is available
    pub fn has_entra(&self) -> bool {
        self.entra.is_some()
    }
}

/// Test result for authentication
#[derive(Debug, Clone, Serialize)]
pub struct AuthTestResult {
    pub method: String,
    pub success: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_auth() {
        let auth = ApiKeyAuth::new("test-key".to_string());
        assert_eq!(auth.method_name(), "API Key");
    }

    #[tokio::test]
    async fn test_api_key_credentials() {
        let auth = ApiKeyAuth::new("test-key".to_string());
        let creds = auth.get_credentials().await.unwrap();
        match creds {
            Credentials::ApiKey(key) => assert_eq!(key, "test-key"),
            _ => panic!("Expected API key credentials"),
        }
    }
}
