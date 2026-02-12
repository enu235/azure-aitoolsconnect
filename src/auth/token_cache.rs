use crate::error::{AppError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single cached token entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedTokenEntry {
    pub access_token: String,
    pub expires_at: DateTime<Utc>,
    pub scope: String,
    pub tenant_id: String,
}

impl CachedTokenEntry {
    /// Check if this token is still valid (with 60-second buffer)
    pub fn is_valid(&self) -> bool {
        Utc::now() + chrono::Duration::seconds(60) < self.expires_at
    }

    /// Get remaining validity in minutes
    pub fn remaining_minutes(&self) -> i64 {
        let remaining = self.expires_at - Utc::now();
        remaining.num_minutes()
    }
}

/// On-disk token cache file
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TokenCacheFile {
    pub tokens: Vec<CachedTokenEntry>,
}

impl TokenCacheFile {
    /// Get the platform-specific cache directory
    pub fn cache_dir() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            std::env::var("LOCALAPPDATA")
                .ok()
                .map(|p| PathBuf::from(p).join("azure-aitoolsconnect"))
        }

        #[cfg(not(target_os = "windows"))]
        {
            std::env::var("HOME")
                .ok()
                .map(|p| PathBuf::from(p).join(".cache").join("azure-aitoolsconnect"))
        }
    }

    /// Get the full path to the cache file
    fn cache_file_path() -> Option<PathBuf> {
        Self::cache_dir().map(|d| d.join("tokens.json"))
    }

    /// Load the token cache from disk (returns empty cache if file doesn't exist)
    pub fn load() -> Result<Self> {
        let path = match Self::cache_file_path() {
            Some(p) => p,
            None => return Ok(Self::default()),
        };

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path).map_err(|e| {
            AppError::Config(format!("Failed to read token cache: {}", e))
        })?;

        let mut cache: TokenCacheFile = serde_json::from_str(&content).map_err(|e| {
            AppError::Config(format!("Failed to parse token cache: {}", e))
        })?;

        // Prune expired tokens on load
        cache.tokens.retain(|t| t.is_valid());

        Ok(cache)
    }

    /// Save the token cache to disk
    pub fn save(&self) -> Result<()> {
        let dir = match Self::cache_dir() {
            Some(d) => d,
            None => return Err(AppError::Config("Cannot determine cache directory".to_string())),
        };

        // Create cache directory if it doesn't exist
        std::fs::create_dir_all(&dir).map_err(|e| {
            AppError::Config(format!("Failed to create cache directory: {}", e))
        })?;

        let path = dir.join("tokens.json");
        let content = serde_json::to_string_pretty(self).map_err(|e| {
            AppError::Config(format!("Failed to serialize token cache: {}", e))
        })?;

        std::fs::write(&path, &content).map_err(|e| {
            AppError::Config(format!("Failed to write token cache: {}", e))
        })?;

        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&path, perms).map_err(|e| {
                AppError::Config(format!("Failed to set cache file permissions: {}", e))
            })?;
        }

        Ok(())
    }

    /// Get a valid cached token for the given scope and tenant
    pub fn get_valid_token(&self, scope: &str, tenant_id: &str) -> Option<&CachedTokenEntry> {
        self.tokens.iter().find(|t| {
            t.scope == scope && t.tenant_id == tenant_id && t.is_valid()
        })
    }

    /// Insert or update a token entry (replaces existing entry for same scope+tenant)
    pub fn insert(&mut self, entry: CachedTokenEntry) {
        self.tokens.retain(|t| {
            !(t.scope == entry.scope && t.tenant_id == entry.tenant_id)
        });
        self.tokens.push(entry);
    }

    /// Clear all cached tokens
    pub fn clear() -> Result<()> {
        if let Some(path) = Self::cache_file_path() {
            if path.exists() {
                std::fs::remove_file(&path).map_err(|e| {
                    AppError::Config(format!("Failed to remove token cache: {}", e))
                })?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cached_token_valid() {
        let entry = CachedTokenEntry {
            access_token: "test".to_string(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            scope: "scope".to_string(),
            tenant_id: "tenant".to_string(),
        };
        assert!(entry.is_valid());
        assert!(entry.remaining_minutes() > 50);
    }

    #[test]
    fn test_cached_token_expired() {
        let entry = CachedTokenEntry {
            access_token: "test".to_string(),
            expires_at: Utc::now() - chrono::Duration::hours(1),
            scope: "scope".to_string(),
            tenant_id: "tenant".to_string(),
        };
        assert!(!entry.is_valid());
    }

    #[test]
    fn test_cache_insert_and_lookup() {
        let mut cache = TokenCacheFile::default();
        let entry = CachedTokenEntry {
            access_token: "token1".to_string(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            scope: "scope1".to_string(),
            tenant_id: "tenant1".to_string(),
        };
        cache.insert(entry);
        assert!(cache.get_valid_token("scope1", "tenant1").is_some());
        assert!(cache.get_valid_token("scope2", "tenant1").is_none());
    }

    #[test]
    fn test_cache_insert_replaces_existing() {
        let mut cache = TokenCacheFile::default();
        let entry1 = CachedTokenEntry {
            access_token: "old-token".to_string(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            scope: "scope".to_string(),
            tenant_id: "tenant".to_string(),
        };
        cache.insert(entry1);

        let entry2 = CachedTokenEntry {
            access_token: "new-token".to_string(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            scope: "scope".to_string(),
            tenant_id: "tenant".to_string(),
        };
        cache.insert(entry2);

        assert_eq!(cache.tokens.len(), 1);
        assert_eq!(cache.get_valid_token("scope", "tenant").unwrap().access_token, "new-token");
    }

    #[test]
    fn test_cache_prunes_expired_on_lookup() {
        let cache = TokenCacheFile {
            tokens: vec![CachedTokenEntry {
                access_token: "expired".to_string(),
                expires_at: Utc::now() - chrono::Duration::hours(1),
                scope: "scope".to_string(),
                tenant_id: "tenant".to_string(),
            }],
        };
        assert!(cache.get_valid_token("scope", "tenant").is_none());
    }
}
