use crate::config::{AuthMethod, Cloud, EntraConfig, UserAuthConfig, DEFAULT_TIMEOUT_SECS};
use crate::error::{AppError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

mod device_code;
mod managed_identity;
mod manual_token;
pub mod token_cache;

pub use device_code::{DeviceCodeAuth, TokenResult};
pub use managed_identity::ManagedIdentityAuth;
pub use manual_token::ManualTokenAuth;

/// Token response from Entra ID
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    #[serde(rename = "token_type")]
    _token_type: String,
}

/// Cached token with expiry
#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    expires_at: Instant,
}

impl CachedToken {
    fn is_expired(&self, buffer_secs: u64) -> bool {
        Instant::now() + Duration::from_secs(buffer_secs) >= self.expires_at
    }
}

/// Token expiry buffer to avoid race conditions (consider expired N seconds early)
const TOKEN_EXPIRY_BUFFER_SECS: u64 = 60;

/// Shared token cache that handles caching logic for auth providers
struct TokenCache {
    cached: Arc<RwLock<Option<CachedToken>>>,
    buffer_secs: u64,
}

impl TokenCache {
    fn new(buffer_secs: u64) -> Self {
        Self {
            cached: Arc::new(RwLock::new(None)),
            buffer_secs,
        }
    }

    async fn get(&self) -> Option<String> {
        let cache = self.cached.read().await;
        if let Some(ref cached) = *cache {
            if !cached.is_expired(self.buffer_secs) {
                return Some(cached.token.clone());
            }
        }
        None
    }

    async fn set(&self, token: String, expires_in_secs: u64) {
        let cached = CachedToken {
            token,
            expires_at: Instant::now() + Duration::from_secs(expires_in_secs),
        };
        let mut cache = self.cached.write().await;
        *cache = Some(cached);
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
    pub fn apply_to_request(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self {
            Credentials::ApiKey(key) => request.header("Ocp-Apim-Subscription-Key", key),
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
    token_cache: TokenCache,
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
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| AppError::Network(e.to_string()))?;

        Ok(Self {
            client,
            cloud,
            tenant_id,
            client_id,
            client_secret,
            scope: cloud.cognitive_scope().to_string(),
            token_cache: TokenCache::new(TOKEN_EXPIRY_BUFFER_SECS),
        })
    }

    async fn fetch_token(&self) -> Result<(String, u64)> {
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

        Ok((token_response.access_token, token_response.expires_in))
    }
}

#[async_trait::async_trait]
impl AuthProvider for EntraTokenAuth {
    async fn get_credentials(&self) -> Result<Credentials> {
        // Check cache first
        if let Some(token) = self.token_cache.get().await {
            return Ok(Credentials::BearerToken(token));
        }

        // Fetch new token
        let (token, expires_in) = self.fetch_token().await?;

        // Update cache
        self.token_cache.set(token.clone(), expires_in).await;

        Ok(Credentials::BearerToken(token))
    }

    fn method_name(&self) -> &'static str {
        "Entra ID Token"
    }
}

/// Cognitive token lifetime (10 minutes, but we use 9 to be safe)
const COGNITIVE_TOKEN_LIFETIME_SECS: u64 = 540;

/// Cognitive Services token exchange (API key -> short-lived token)
pub struct CognitiveTokenAuth {
    client: Client,
    api_key: String,
    token_endpoint: String,
    token_cache: TokenCache,
}

impl CognitiveTokenAuth {
    pub fn new(api_key: String, region: &str, cloud: Cloud) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| AppError::Network(e.to_string()))?;

        let token_endpoint = cloud.cognitive_token_endpoint(region);

        Ok(Self {
            client,
            api_key,
            token_endpoint,
            token_cache: TokenCache::new(TOKEN_EXPIRY_BUFFER_SECS),
        })
    }

    pub async fn exchange_token(&self) -> Result<String> {
        // Check cache first
        if let Some(token) = self.token_cache.get().await {
            return Ok(token);
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

        // Update cache
        self.token_cache
            .set(token.clone(), COGNITIVE_TOKEN_LIFETIME_SECS)
            .await;

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
    manual_token: Option<ManualTokenAuth>,
    managed_identity: Option<ManagedIdentityAuth>,
    device_code: Option<DeviceCodeAuth>,
    default_method: AuthMethod,
}

impl AuthManager {
    pub fn new(
        api_key: Option<String>,
        entra_config: Option<&EntraConfig>,
        user_config: Option<&UserAuthConfig>,
        cloud: Cloud,
        default_method: AuthMethod,
    ) -> Result<Self> {
        Self::new_with_options(
            api_key,
            entra_config,
            user_config,
            cloud,
            default_method,
            false,
        )
    }

    pub fn new_with_options(
        api_key: Option<String>,
        entra_config: Option<&EntraConfig>,
        user_config: Option<&UserAuthConfig>,
        cloud: Cloud,
        default_method: AuthMethod,
        quiet: bool,
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

        // Initialize manual token auth if bearer token is provided
        let manual_token_auth = if let Some(config) = user_config {
            if let Some(token) = &config.bearer_token {
                Some(ManualTokenAuth::new(token.clone())?)
            } else {
                None
            }
        } else {
            None
        };

        // Initialize managed identity auth if requested
        let managed_identity_auth = if default_method == AuthMethod::ManagedIdentity {
            let user_assigned_client_id =
                user_config.and_then(|c| c.managed_identity_client_id.clone());
            Some(ManagedIdentityAuth::new(&cloud, user_assigned_client_id)?)
        } else {
            None
        };

        // Initialize device code auth if requested
        let device_code_auth = if default_method == AuthMethod::DeviceCode {
            let tenant_id = user_config
                .and_then(|c| c.tenant_id.clone())
                .ok_or(AppError::MissingTenantId)?;
            let client_id = user_config.and_then(|c| c.client_id.clone());
            Some(DeviceCodeAuth::new(tenant_id, client_id, &cloud)?.with_quiet(quiet))
        } else {
            None
        };

        Ok(Self {
            api_key: api_key_auth,
            entra: entra_auth,
            manual_token: manual_token_auth,
            managed_identity: managed_identity_auth,
            device_code: device_code_auth,
            default_method,
        })
    }

    /// Get the primary auth provider based on configuration
    pub fn get_provider(&self) -> Result<&dyn AuthProvider> {
        match self.default_method {
            AuthMethod::Key => {
                if let Some(ref api_key) = self.api_key {
                    Ok(api_key as &dyn AuthProvider)
                } else {
                    Err(AppError::Config("API key not configured".to_string()))
                }
            }
            AuthMethod::Token => {
                if let Some(ref entra) = self.entra {
                    Ok(entra as &dyn AuthProvider)
                } else {
                    Err(AppError::Config(
                        "Entra token auth not configured".to_string(),
                    ))
                }
            }
            AuthMethod::DeviceCode => {
                if let Some(ref device_code) = self.device_code {
                    Ok(device_code as &dyn AuthProvider)
                } else {
                    Err(AppError::MissingTenantId)
                }
            }
            AuthMethod::ManagedIdentity => {
                if let Some(ref mi) = self.managed_identity {
                    Ok(mi as &dyn AuthProvider)
                } else {
                    Err(AppError::ManagedIdentityNotAvailable(
                        "Managed identity not available in this environment".to_string(),
                    ))
                }
            }
            AuthMethod::ManualToken => {
                if let Some(ref manual) = self.manual_token {
                    Ok(manual as &dyn AuthProvider)
                } else {
                    Err(AppError::InvalidBearerToken(
                        "Bearer token not provided".to_string(),
                    ))
                }
            }
            AuthMethod::Both => {
                // Try API key first, fallback to entra
                if let Some(ref key) = self.api_key {
                    Ok(key as &dyn AuthProvider)
                } else if let Some(ref entra) = self.entra {
                    Ok(entra as &dyn AuthProvider)
                } else {
                    Err(AppError::Config("No authentication configured".to_string()))
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
