use crate::error::{AppError, AppResult};
use crate::schema::FieldMapping;
use chrono::{DateTime, Utc};
use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Registry {
    pub version: u32,
    pub sites: HashMap<String, RegisteredSite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredSite {
    pub url: String,
    pub name: String,
    pub indices: Vec<AlgoliaIndex>,
    pub discovered_at: DateTime<Utc>,
    pub last_verified: DateTime<Utc>,
    pub discovery_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgoliaIndex {
    pub app_id: String,
    pub api_key: String,
    pub index_name: String,
    pub record_count: Option<u64>,
    pub facets: Option<HashMap<String, Vec<String>>>,
    pub is_default: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_mapping: Option<FieldMapping>,
}

impl Registry {
    pub fn load(path: &Path) -> AppResult<Self> {
        if !path.exists() {
            return Ok(Registry {
                version: 1,
                sites: HashMap::new(),
            });
        }

        let lock_path = path.with_extension("lock");
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;

        lock_file
            .lock_shared()
            .map_err(|e| AppError::LockContention(e.to_string()))?;

        let contents = std::fs::read_to_string(path)?;
        let registry: Registry = serde_json::from_str(&contents)
            .map_err(|e| AppError::Registry(format!("corrupt registry: {}", e)))?;

        lock_file
            .unlock()
            .map_err(|e| AppError::Registry(format!("unlock failed: {}", e)))?;

        Ok(registry)
    }

    pub fn save(&self, path: &Path) -> AppResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let lock_path = path.with_extension("lock");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;

        lock_file
            .lock_exclusive()
            .map_err(|e| AppError::LockContention(e.to_string()))?;

        let serialized = serde_json::to_string_pretty(self)
            .map_err(|e| AppError::Registry(format!("serialization failed: {}", e)))?;

        // Atomic write: write to temp, then rename
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, &serialized)?;
        std::fs::rename(&tmp_path, path)?;

        lock_file
            .unlock()
            .map_err(|e| AppError::Registry(format!("unlock failed: {}", e)))?;

        Ok(())
    }

    pub fn add_site(&mut self, name: String, site: RegisteredSite) {
        self.sites.insert(name, site);
    }

    pub fn remove_site(&mut self, name: &str) -> Option<RegisteredSite> {
        self.sites.remove(name)
    }

    pub fn get_site(&self, name: &str) -> Option<&RegisteredSite> {
        self.sites.get(name)
    }
}

/// Derive a short site name from a URL.
/// e.g., "https://react.dev" -> "react"
///       "https://docs.astro.build" -> "astro"
///       "https://tailwindcss.com/docs" -> "tailwindcss"
pub fn derive_site_name(url_str: &str) -> Option<String> {
    let parsed = url::Url::parse(url_str).ok()?;
    let host = parsed.host_str()?;

    // Split hostname into parts
    let parts: Vec<&str> = host.split('.').collect();

    if parts.len() >= 2 {
        // Strip common prefixes like "docs."
        let name = if parts[0] == "docs" && parts.len() >= 3 {
            parts[1].to_string()
        } else {
            // Use the main domain name (second-to-last for .com/.dev/.org, or first part for two-part domains)
            if parts.len() == 2 {
                parts[0].to_string()
            } else {
                // e.g., tailwindcss.com -> tailwindcss
                parts[parts.len() - 2].to_string()
            }
        };

        Some(name)
    } else {
        Some(host.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_name_simple_domain() {
        assert_eq!(derive_site_name("https://react.dev"), Some("react".to_string()));
    }

    #[test]
    fn test_derive_name_docs_prefix() {
        assert_eq!(derive_site_name("https://docs.astro.build"), Some("astro".to_string()));
    }

    #[test]
    fn test_derive_name_com_domain() {
        assert_eq!(
            derive_site_name("https://tailwindcss.com/docs"),
            Some("tailwindcss".to_string())
        );
    }

    #[test]
    fn test_derive_name_with_path() {
        assert_eq!(
            derive_site_name("https://vuejs.org/guide"),
            Some("vuejs".to_string())
        );
    }

    #[test]
    fn test_derive_name_www() {
        assert_eq!(
            derive_site_name("https://www.1000farmacie.it"),
            Some("1000farmacie".to_string())
        );
    }

    #[test]
    fn test_derive_name_invalid_url() {
        assert_eq!(derive_site_name("not-a-url"), None);
    }
}
