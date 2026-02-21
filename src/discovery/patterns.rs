use regex::Regex;
use std::sync::LazyLock;

/// Algolia application ID: exactly 10 uppercase alphanumeric chars.
pub static APP_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Z0-9]{10}").unwrap());

/// Legacy DocSearch API key: exactly 32 lowercase hex chars.
pub static LEGACY_KEY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-f0-9]{32}").unwrap());

/// Modern Algolia API key: 20-64 mixed alphanumeric chars.
/// Must contain at least one letter and one digit (to exclude plain words or numbers).
pub static MODERN_KEY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-zA-Z0-9]{20,64}").unwrap());

/// Known property names that hold API keys.
pub const API_KEY_PROPERTIES: &[&str] = &[
    "apiKey",
    "apikey",
    "api_key",
    "api-key",
    "searchApiKey",
    "search_api_key",
    "searchKey",
    "algoliaApiKey",
    "algolia_api_key",
    "searchOnlyApiKey",
    "ALGOLIA_SEARCH_API_KEY",
    "ALGOLIA_API_KEY",
];

/// Known property names that hold application IDs.
pub const APP_ID_PROPERTIES: &[&str] = &[
    "appId",
    "appid",
    "app_id",
    "app-id",
    "applicationId",
    "application_id",
    "algoliaAppId",
    "algolia_app_id",
    "algoliaApplicationId",
    "ALGOLIA_APPLICATION_ID",
    "ALGOLIA_APP_ID",
];

/// Known property names that hold index names.
pub const INDEX_NAME_PROPERTIES: &[&str] = &[
    "indexName",
    "indexname",
    "index_name",
    "index-name",
    "algoliaIndex",
    "algolia_index",
    "productsAlgoliaIndex",
    "allOffersAlgoliaIndex",
    "suggestProductsAlgoliaIndex",
    "ALGOLIA_INDEX_NAME",
];

/// Check if a potential modern key has mixed content (not all-alpha, not all-digit).
pub fn is_mixed_alphanumeric(s: &str) -> bool {
    let has_digit = s.chars().any(|c| c.is_ascii_digit());
    let has_alpha = s.chars().any(|c| c.is_ascii_alphabetic());
    has_digit && has_alpha
}

/// Score a potential API key based on surrounding context.
/// Higher score = more confident this is a real API key.
pub fn context_score(text: &str, key_offset: usize, key_len: usize) -> u8 {
    let mut score: u8 = 0;

    // Check if there's an app ID pattern within 500 chars
    let window_start = key_offset.saturating_sub(500);
    let window_end = (key_offset + key_len + 500).min(text.len());
    let window = &text[window_start..window_end];

    if APP_ID_RE.is_match(window) {
        score += 3;
    }

    // Check if preceded by a known property name
    let prefix_start = key_offset.saturating_sub(50);
    let prefix = &text[prefix_start..key_offset];
    let prefix_lower = prefix.to_lowercase();

    for prop in API_KEY_PROPERTIES {
        if prefix_lower.contains(&prop.to_lowercase()) {
            score += 3;
            break;
        }
    }

    // Check for algolia/docsearch context in surrounding 2000 chars
    let ctx_start = key_offset.saturating_sub(1000);
    let ctx_end = (key_offset + key_len + 1000).min(text.len());
    let ctx = &text[ctx_start..ctx_end].to_lowercase();

    if ctx.contains("algolia") || ctx.contains("docsearch") {
        score += 2;
    }

    score
}

/// Minimum confidence score for a modern key to be considered a candidate.
pub const MIN_CONFIDENCE: u8 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_id_regex_matches_valid() {
        assert!(APP_ID_RE.is_match("BH4D9OD16A"));
        assert!(APP_ID_RE.is_match("X1Z85QJPUV"));
        assert!(APP_ID_RE.is_match("1FCF9AYYAT"));
    }

    #[test]
    fn test_app_id_regex_rejects_lowercase() {
        // Should not match a purely lowercase string of 10 chars
        assert!(!APP_ID_RE.is_match("abcdefghij"));
    }

    #[test]
    fn test_legacy_key_matches_32_hex() {
        assert!(LEGACY_KEY_RE.is_match("d9aa2d7a17b51cc4b053e1ee0bd1d4b5"));
        assert!(LEGACY_KEY_RE.is_match("1be39e4a1d73c3b1e1082e3c1c5b263d"));
    }

    #[test]
    fn test_legacy_key_rejects_short() {
        assert!(!LEGACY_KEY_RE.is_match("d9aa2d7a17b51cc4b053"));
    }

    #[test]
    fn test_modern_key_matches_long_alphanumeric() {
        assert!(MODERN_KEY_RE.is_match("NsQ2e7CPXieEfGYLCVpbVICBTFGa12VD"));
    }

    #[test]
    fn test_is_mixed_alphanumeric() {
        assert!(is_mixed_alphanumeric("abc123def"));
        assert!(!is_mixed_alphanumeric("abcdefgh"));
        assert!(!is_mixed_alphanumeric("12345678"));
    }

    #[test]
    fn test_context_score_near_app_id() {
        let text = "var appId='BH4D9OD16A'; var apiKey='NsQ2e7CPXieEfGYLCVpbVICBTFGa12VD';";
        // The key starts at index 42
        let key_offset = text.find("NsQ2e7").unwrap();
        let score = context_score(text, key_offset, 32);
        // Should have: +3 for nearby app_id, +3 for apiKey property name
        assert!(score >= 3);
    }

    #[test]
    fn test_context_score_isolated_string() {
        // No algolia context, no app_id nearby
        let text = "var something = 'NsQ2e7CPXieEfGYLCVpbVICBTFGa12VD';";
        let key_offset = text.find("NsQ2e7").unwrap();
        let score = context_score(text, key_offset, 32);
        assert!(score < MIN_CONFIDENCE);
    }

    #[test]
    fn test_context_score_algolia_context() {
        let text = "algolia: { key: 'NsQ2e7CPXieEfGYLCVpbVICBTFGa12VD' }";
        let key_offset = text.find("NsQ2e7").unwrap();
        let score = context_score(text, key_offset, 32);
        // Should get +2 for algolia context
        assert!(score >= 2);
    }
}
