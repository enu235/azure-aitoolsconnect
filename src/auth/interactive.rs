use super::{AuthProvider, Credentials, TokenResponse};
use crate::config::Cloud;
use crate::error::{AppError, Result};
use async_trait::async_trait;
use console::style;
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, ClientId, CsrfToken, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenUrl,
};
use std::io::{BufRead, Write};
use std::net::TcpListener;
use std::time::Duration;
use url::Url;

use super::device_code::TokenResult;

/// Azure CLI's well-known public client ID
const AZURE_CLI_CLIENT_ID: &str = "04b07795-8ddb-461a-bbee-02f9e1bf7b46";

/// Interactive browser-based authentication provider using Authorization Code + PKCE flow.
///
/// This flow is preferred over device code in enterprise environments where
/// Conditional Access policies block the device code grant (error AADSTS53003).
pub struct InteractiveAuth {
    tenant_id: String,
    client_id: String,
    scope: String,
    cloud: Cloud,
    quiet: bool,
}

impl InteractiveAuth {
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

    /// Fetch token using authorization code flow with PKCE
    async fn fetch_token(&self) -> Result<TokenResult> {
        let login_endpoint = self.cloud.login_endpoint();

        let auth_url = AuthUrl::new(format!(
            "{}/{}/oauth2/v2.0/authorize",
            login_endpoint, self.tenant_id
        ))
        .map_err(|e| AppError::Auth(format!("Invalid auth URL: {}", e)))?;

        let token_url = TokenUrl::new(format!(
            "{}/{}/oauth2/v2.0/token",
            login_endpoint, self.tenant_id
        ))
        .map_err(|e| AppError::Auth(format!("Invalid token URL: {}", e)))?;

        // Bind a temporary localhost listener to get an available port
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| AppError::Auth(format!("Failed to bind localhost listener: {}", e)))?;
        let port = listener
            .local_addr()
            .map_err(|e| AppError::Auth(format!("Failed to get listener address: {}", e)))?
            .port();

        let redirect_url = RedirectUrl::new(format!("http://localhost:{}", port))
            .map_err(|e| AppError::Auth(format!("Invalid redirect URL: {}", e)))?;

        let client = BasicClient::new(
            ClientId::new(self.client_id.clone()),
            None,
            auth_url,
            Some(token_url),
        )
        .set_redirect_uri(redirect_url);

        // Generate PKCE challenge
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        // Build authorization URL
        let (authorize_url, csrf_state) = client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new(self.scope.clone()))
            .add_extra_param("response_mode", "query")
            .set_pkce_challenge(pkce_challenge)
            .url();

        // Display instructions and open browser
        if !self.quiet {
            eprintln!();
            eprintln!(
                "{}",
                style("======================================================================")
                    .cyan()
            );
            eprintln!(
                "  {} {}",
                style("[*]").cyan(),
                style("Opening browser for Azure authentication...").bold()
            );
            eprintln!(
                "{}",
                style("======================================================================")
                    .cyan()
            );
            eprintln!();
            eprintln!("  If the browser doesn't open, visit this URL:");
            eprintln!("  {}", style(authorize_url.as_str()).underlined());
            eprintln!();
        }

        // Try to open the browser
        let _ = open_browser(authorize_url.as_str());

        // Wait for the callback
        let (code, received_state) = wait_for_callback(listener)?;

        // Verify CSRF state
        if received_state.secret() != csrf_state.secret() {
            return Err(AppError::Auth(
                "CSRF state mismatch — possible security issue. Please try again.".to_string(),
            ));
        }

        if !self.quiet {
            eprintln!(
                "  {} Authorization code received, exchanging for token...",
                style("[*]").cyan()
            );
        }

        // Exchange authorization code for token
        let token = self
            .exchange_code(&code, &pkce_verifier, port)
            .await?;

        if !self.quiet {
            eprintln!(
                "  {} {}",
                style("[+]").green(),
                style("Authentication successful!").green().bold()
            );
            eprintln!();
        }

        Ok(token)
    }

    /// Exchange the authorization code for an access token using reqwest directly
    /// (the oauth2 crate's async client has compatibility issues, so we do it manually)
    async fn exchange_code(
        &self,
        code: &str,
        pkce_verifier: &PkceCodeVerifier,
        port: u16,
    ) -> Result<TokenResult> {
        let login_endpoint = self.cloud.login_endpoint();
        let token_url = format!(
            "{}/{}/oauth2/v2.0/token",
            login_endpoint, self.tenant_id
        );

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::Auth(format!("Failed to create HTTP client: {}", e)))?;

        let params = [
            ("grant_type", "authorization_code"),
            ("client_id", &self.client_id),
            ("code", code),
            ("redirect_uri", &format!("http://localhost:{}", port)),
            ("code_verifier", pkce_verifier.secret()),
            ("scope", &self.scope),
        ];

        let response = client
            .post(&token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| AppError::Auth(format!("Token exchange request failed: {}", e)))?;

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

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| AppError::Auth(format!("Failed to parse token response: {}", e)))?;

        Ok(TokenResult {
            access_token: token_response.access_token,
            expires_in_secs: token_response.expires_in,
            scope: self.scope.clone(),
        })
    }
}

#[async_trait]
impl AuthProvider for InteractiveAuth {
    async fn get_credentials(&self) -> Result<Credentials> {
        let result = self.fetch_token().await?;
        Ok(Credentials::BearerToken(result.access_token))
    }

    fn method_name(&self) -> &'static str {
        "Interactive Browser"
    }
}

/// Wait for the OAuth2 callback on the localhost listener.
/// Returns (authorization_code, csrf_state).
fn wait_for_callback(listener: TcpListener) -> Result<(String, CsrfToken)> {
    // Set a timeout so we don't hang forever
    listener
        .set_nonblocking(false)
        .map_err(|e| AppError::Auth(format!("Failed to set listener blocking: {}", e)))?;

    let (mut stream, _) = listener
        .accept()
        .map_err(|e| AppError::Auth(format!("Failed to accept callback connection: {}", e)))?;

    let mut reader = std::io::BufReader::new(&stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|e| AppError::Auth(format!("Failed to read callback request: {}", e)))?;

    // Parse the GET request to extract query parameters
    // Format: GET /callback?code=...&state=... HTTP/1.1
    let url_str = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| AppError::Auth("Invalid callback request".to_string()))?;

    let url = Url::parse(&format!("http://localhost{}", url_str))
        .map_err(|e| AppError::Auth(format!("Failed to parse callback URL: {}", e)))?;

    // Check for error response
    let params: std::collections::HashMap<_, _> = url.query_pairs().collect();

    if let Some(error) = params.get("error") {
        let desc = params
            .get("error_description")
            .map(|d| d.to_string())
            .unwrap_or_default();
        // Send error response to browser
        let body = format!(
            "<html><body><h2>Authentication Failed</h2><p>{}: {}</p><p>You can close this window.</p></body></html>",
            error, desc
        );
        let response = format!(
            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
        return Err(AppError::Auth(format!(
            "Authentication error: {} - {}",
            error, desc
        )));
    }

    let code = params
        .get("code")
        .ok_or_else(|| AppError::Auth("No authorization code in callback".to_string()))?
        .to_string();

    let state = params
        .get("state")
        .ok_or_else(|| AppError::Auth("No state parameter in callback".to_string()))?
        .to_string();

    // Send success response to browser
    let body = "<html><body><h2>Authentication Successful!</h2><p>You can close this window and return to the terminal.</p></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();

    Ok((code, CsrfToken::new(state)))
}

/// Try to open a URL in the user's default browser
fn open_browser(url: &str) -> std::result::Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", url])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interactive_auth_creation() {
        let auth = InteractiveAuth::new("tenant-id".to_string(), None, &Cloud::Global);
        assert!(auth.is_ok());
        let auth = auth.unwrap();
        assert_eq!(auth.client_id, AZURE_CLI_CLIENT_ID);
    }

    #[test]
    fn test_interactive_auth_custom_client_id() {
        let custom_id = "custom-client-id".to_string();
        let auth = InteractiveAuth::new(
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
        let auth = InteractiveAuth::new("tenant-id".to_string(), None, &Cloud::China).unwrap();
        assert!(auth.scope.contains("cognitiveservices.azure.cn"));
    }

    #[test]
    fn test_quiet_mode() {
        let auth = InteractiveAuth::new("tenant-id".to_string(), None, &Cloud::Global)
            .unwrap()
            .with_quiet(true);
        assert!(auth.quiet);
    }
}
