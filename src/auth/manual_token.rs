use super::{AuthProvider, Credentials};
use crate::error::{AppError, Result};
use async_trait::async_trait;

/// Token authentication provider
/// Accepts a pre-obtained bearer token and returns it for authentication
pub struct ManualTokenAuth {
    token: String,
}

impl ManualTokenAuth {
    pub fn new(token: String) -> Result<Self> {
        // Basic validation: not empty, reasonable length
        if token.trim().is_empty() {
            return Err(AppError::InvalidBearerToken(
                "Token cannot be empty".to_string(),
            ));
        }

        // Basic sanity check for token length (JWT tokens are typically > 50 chars)
        if token.len() < 20 {
            return Err(AppError::InvalidBearerToken(
                "Token appears to be too short".to_string(),
            ));
        }

        Ok(Self { token })
    }
}

#[async_trait]
impl AuthProvider for ManualTokenAuth {
    async fn get_credentials(&self) -> Result<Credentials> {
        Ok(Credentials::BearerToken(self.token.clone()))
    }

    fn method_name(&self) -> &'static str {
        "Token"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_token_rejected() {
        let result = ManualTokenAuth::new("".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_short_token_rejected() {
        let result = ManualTokenAuth::new("short".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_token_accepted() {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ";
        let result = ManualTokenAuth::new(token.to_string());
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_credentials() {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ";
        let auth = ManualTokenAuth::new(token.to_string()).unwrap();
        let creds = auth.get_credentials().await.unwrap();
        match creds {
            Credentials::BearerToken(t) => assert_eq!(t, token),
            _ => panic!("Expected bearer token credentials"),
        }
    }
}
