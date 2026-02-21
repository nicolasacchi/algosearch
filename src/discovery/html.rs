use crate::discovery::extraction::{extract_brace_balanced, extract_kv_from_js_object};
use crate::discovery::patterns::{
    self, APP_ID_PROPERTIES, API_KEY_PROPERTIES, INDEX_NAME_PROPERTIES,
};
use crate::discovery::CandidateCredentials;
use regex::Regex;
use std::sync::LazyLock;

/// Run all HTML scanning strategies and collect candidate credentials.
pub fn scan_html(html: &str) -> Vec<CandidateCredentials> {
    let mut candidates = Vec::new();

    candidates.extend(scan_docsearch_init(html));
    candidates.extend(scan_algoliasearch_init(html));
    candidates.extend(scan_meta_tags(html));
    candidates.extend(scan_framework_hydration(html));
    candidates.extend(scan_generic_config_objects(html));
    candidates.extend(scan_escaped_json(html));
    candidates.extend(scan_window_globals(html));
    candidates.extend(scan_preconnect_hints(html));
    candidates.extend(scan_proximity_patterns(html));

    candidates
}

// Strategy 1a: DocSearch initialization pattern
// docsearch({appId: '...', apiKey: '...', indexName: '...'})
static DOCSEARCH_CALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:docsearch|DocSearch)\s*\(").unwrap()
});

fn scan_docsearch_init(html: &str) -> Vec<CandidateCredentials> {
    let mut results = Vec::new();

    for m in DOCSEARCH_CALL_RE.find_iter(html) {
        let rest = &html[m.end()..];
        if let Some(obj) = extract_brace_balanced(rest) {
            let kv = extract_kv_from_js_object(obj);
            if let Some(cred) = credentials_from_kv(&kv) {
                results.push(cred);
            }
        }
    }

    results
}

// Strategy 1b: Algolia client initialization
// algoliasearch('APPID', 'APIKEY')
static ALGOLIA_CLIENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"algoliasearch\s*\(\s*['"`]([A-Z0-9]{10})['"`]\s*,\s*['"`]([a-zA-Z0-9]{20,64})['"`]"#).unwrap()
});

fn scan_algoliasearch_init(html: &str) -> Vec<CandidateCredentials> {
    let mut results = Vec::new();

    for cap in ALGOLIA_CLIENT_RE.captures_iter(html) {
        if let (Some(app_id), Some(api_key)) = (cap.get(1), cap.get(2)) {
            // We have app_id and api_key but need to find index_name nearby
            let offset = cap.get(0).unwrap().end();
            if let Some(index_name) = find_index_name_nearby(html, offset) {
                results.push(CandidateCredentials {
                    app_id: app_id.as_str().to_string(),
                    api_key: api_key.as_str().to_string(),
                    index_name,
                });
            }
        }
    }

    results
}

// Strategy 1c: Meta tags
fn scan_meta_tags(html: &str) -> Vec<CandidateCredentials> {
    let mut app_id = None;
    let mut api_key = None;
    let mut index_name = None;

    // Use scraper for proper HTML parsing
    {
        let doc = scraper::Html::parse_document(html);

        // Check meta tags
        let meta_sel = scraper::Selector::parse("meta").unwrap();
        for elem in doc.select(&meta_sel) {
            let name = elem.value().attr("name").unwrap_or("");
            let content = elem.value().attr("content").unwrap_or("");

            match name {
                "docsearch:app_id" | "docsearch:appId" => app_id = Some(content.to_string()),
                "docsearch:api_key" | "docsearch:apiKey" => api_key = Some(content.to_string()),
                "docsearch:index_name" | "docsearch:indexName" => {
                    index_name = Some(content.to_string())
                }
                _ => {}
            }
        }

        // Check data-docsearch-* attributes on any element
        if app_id.is_none() || api_key.is_none() || index_name.is_none() {
            let all_sel = scraper::Selector::parse("*").unwrap();
            for elem in doc.select(&all_sel) {
                if app_id.is_none() {
                    if let Some(v) = elem.value().attr("data-docsearch-app-id") {
                        app_id = Some(v.to_string());
                    }
                }
                if api_key.is_none() {
                    if let Some(v) = elem.value().attr("data-docsearch-api-key") {
                        api_key = Some(v.to_string());
                    }
                }
                if index_name.is_none() {
                    if let Some(v) = elem.value().attr("data-docsearch-index-name") {
                        index_name = Some(v.to_string());
                    }
                }
            }
        }
    }

    match (app_id, api_key, index_name) {
        (Some(a), Some(k), Some(i)) if !a.is_empty() && !k.is_empty() && !i.is_empty() => {
            vec![CandidateCredentials {
                app_id: a,
                api_key: k,
                index_name: i,
            }]
        }
        _ => vec![],
    }
}

// Strategy 1d: Framework hydration blobs (__NEXT_DATA__, __NUXT__, etc.)
static NEXT_DATA_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<script\s+id="__NEXT_DATA__"[^>]*>([\s\S]*?)</script>"#).unwrap()
});

static NUXT_DATA_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"window\.__NUXT__\s*=\s*(\{[\s\S]*?\})\s*[;<]"#).unwrap()
});

fn scan_framework_hydration(html: &str) -> Vec<CandidateCredentials> {
    let mut results = Vec::new();

    // Next.js __NEXT_DATA__
    for cap in NEXT_DATA_RE.captures_iter(html) {
        if let Some(json_str) = cap.get(1) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str.as_str()) {
                if let Some(cred) = extract_algolia_from_json(&value) {
                    results.push(cred);
                }
            }
        }
    }

    // Nuxt __NUXT__
    for cap in NUXT_DATA_RE.captures_iter(html) {
        if let Some(json_str) = cap.get(1) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str.as_str()) {
                if let Some(cred) = extract_algolia_from_json(&value) {
                    results.push(cred);
                }
            }
        }
    }

    results
}

// Strategy 1e & 1f: Generic config objects (algolia: {, searchConfig: {, etc.)
static CONFIG_OBJECT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:algolia|searchConfig|search|docsearch)\s*[:=]\s*\{").unwrap()
});

fn scan_generic_config_objects(html: &str) -> Vec<CandidateCredentials> {
    let mut results = Vec::new();

    for m in CONFIG_OBJECT_RE.find_iter(html) {
        let obj_start = &html[m.start()..];
        // Find the { within the match
        if let Some(brace_pos) = obj_start.find('{') {
            if let Some(obj) = extract_brace_balanced(&obj_start[brace_pos..]) {
                let kv = extract_kv_from_js_object(obj);
                if let Some(cred) = credentials_from_kv(&kv) {
                    results.push(cred);
                }
            }
        }
    }

    results
}

// Strategy: Escaped JSON blobs (React Server Components, Next.js RSC payloads)
// Pattern: algoliaConfig\":{\"algoliaApiKey\":\"...\",\"algoliaApplicationId\":\"...\"}
// These appear in server-rendered HTML where JSON is double-escaped.
static ESCAPED_JSON_CONFIG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:algolia(?:Config|Settings)?|searchConfig)\\":\s*\{[^}]*\\""#).unwrap()
});

fn scan_escaped_json(html: &str) -> Vec<CandidateCredentials> {
    let mut results = Vec::new();

    // Find escaped JSON blobs containing algolia config
    for m in ESCAPED_JSON_CONFIG_RE.find_iter(html) {
        // Extract a wider window around the match to capture the full config object
        let start = m.start();
        let window_end = (start + 2000).min(html.len());
        let window = &html[start..window_end];

        // Unescape the JSON by replacing \" with "
        let unescaped = window.replace("\\\"", "\"");

        // Try to find the config object within the unescaped text
        if let Some(brace_start) = unescaped.find(":{") {
            let obj_text = &unescaped[brace_start + 1..];
            if let Some(obj) = extract_brace_balanced(obj_text) {
                // Try parsing as JSON
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(obj) {
                    if let Some(cred) = extract_algolia_from_json(&value) {
                        results.push(cred);
                        continue;
                    }
                }
                // Fall back to KV extraction
                let kv = extract_kv_from_js_object(obj);
                if let Some(cred) = credentials_from_kv(&kv) {
                    results.push(cred);
                }
            }
        }
    }

    results
}

// Strategy: window.env / window.config / window.__APP_CONFIG__ global JSON objects
// These often contain ALGOLIA_APPLICATION_ID, ALGOLIA_SEARCH_API_KEY, etc.
static WINDOW_GLOBAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:window\.(?:env|config|__APP_CONFIG__|__ENV__|settings)|globalThis\.env)\s*=\s*\{")
        .unwrap()
});

fn scan_window_globals(html: &str) -> Vec<CandidateCredentials> {
    let mut results = Vec::new();

    for m in WINDOW_GLOBAL_RE.find_iter(html) {
        let obj_start = &html[m.start()..];
        if let Some(brace_pos) = obj_start.find('{') {
            if let Some(obj) = extract_brace_balanced(&obj_start[brace_pos..]) {
                // Try parsing as JSON first (common for window.env = {...})
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(obj) {
                    if let Some(cred) = extract_algolia_from_json_partial(&value) {
                        results.push(cred);
                        continue;
                    }
                }
                // Fall back to JS key-value extraction
                let kv = extract_kv_from_js_object(obj);
                if let Some(cred) = credentials_from_kv(&kv) {
                    results.push(cred);
                }
            }
        }
    }

    results
}

/// Extract partial algolia credentials from a JSON object.
/// Unlike extract_algolia_from_json, this handles cases where the index name
/// might be in a separate JS file (e.g., window.env has appId + apiKey only).
fn extract_algolia_from_json_partial(
    value: &serde_json::Value,
) -> Option<CandidateCredentials> {
    let map = value.as_object()?;

    let app_id = map
        .iter()
        .find(|(k, _)| APP_ID_PROPERTIES.contains(&k.as_str()))
        .and_then(|(_, v)| v.as_str())
        .map(|s| s.to_string())?;

    let api_key = map
        .iter()
        .find(|(k, _)| API_KEY_PROPERTIES.contains(&k.as_str()))
        .and_then(|(_, v)| v.as_str())
        .map(|s| s.to_string())?;

    // Index name may or may not be present
    let index_name = map
        .iter()
        .find(|(k, _)| INDEX_NAME_PROPERTIES.contains(&k.as_str()))
        .and_then(|(_, v)| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    if app_id.is_empty() || api_key.is_empty() {
        return None;
    }

    Some(CandidateCredentials {
        app_id,
        api_key,
        index_name,
    })
}

// Strategy: Extract appId from DNS preconnect hints to Algolia
// e.g. <link rel=preconnect href=https://X1Z85QJPUV-dsn.algolia.net>
static PRECONNECT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)<link[^>]+rel=(?:["']?preconnect["']?)[^>]+href=(?:["']?)https?://([A-Z0-9]{10})-dsn\.algolia(?:search)?\.net"#,
    )
    .unwrap()
});

fn scan_preconnect_hints(html: &str) -> Vec<CandidateCredentials> {
    let mut results = Vec::new();

    for cap in PRECONNECT_RE.captures_iter(html) {
        if let Some(app_id) = cap.get(1) {
            let app_id_str = app_id.as_str().to_string();

            // We have the app_id from preconnect. Now search the entire HTML
            // for an API key and index name in proximity to any algolia-related context.
            if let Some((api_key, index_name)) =
                find_key_and_index_for_app_id(html, &app_id_str)
            {
                results.push(CandidateCredentials {
                    app_id: app_id_str,
                    api_key,
                    index_name,
                });
            }
        }
    }

    results
}

/// Given a known app_id, search the HTML for the corresponding API key and index name.
fn find_key_and_index_for_app_id(html: &str, app_id: &str) -> Option<(String, String)> {
    // Find all occurrences of the app_id in text
    let mut search_start = 0;
    while let Some(pos) = html[search_start..].find(app_id) {
        let abs_pos = search_start + pos;

        // Look for an API key and index name within a window around this occurrence
        let window_start = abs_pos.saturating_sub(1000);
        let window_end = (abs_pos + 1000).min(html.len());
        let window = &html[window_start..window_end];

        // Try legacy key first
        if let Some(key_match) = patterns::LEGACY_KEY_RE.find(window) {
            if let Some(idx) = find_index_name_nearby(html, abs_pos) {
                return Some((key_match.as_str().to_string(), idx));
            }
        }

        // Try modern key with context scoring
        for key_match in patterns::MODERN_KEY_RE.find_iter(window) {
            let key_str = key_match.as_str();
            if patterns::LEGACY_KEY_RE.is_match(key_str) || !patterns::is_mixed_alphanumeric(key_str)
            {
                continue;
            }
            // Lower threshold since we already know the app_id
            let score =
                patterns::context_score(window, key_match.start(), key_str.len());
            if score >= 2 {
                if let Some(idx) = find_index_name_nearby(html, abs_pos) {
                    return Some((key_str.to_string(), idx));
                }
            }
        }

        search_start = abs_pos + app_id.len();
    }

    None
}

// Strategy: Proximity-based scanning for minified code
fn scan_proximity_patterns(html: &str) -> Vec<CandidateCredentials> {
    let mut results = Vec::new();

    // Find all app ID matches
    let app_ids: Vec<(usize, String)> = patterns::APP_ID_RE
        .find_iter(html)
        .map(|m| (m.start(), m.as_str().to_string()))
        .collect();

    // Find all potential API keys (legacy hex)
    for key_match in patterns::LEGACY_KEY_RE.find_iter(html) {
        let key_offset = key_match.start();
        let key_str = key_match.as_str();

        // Find nearest app ID within 500 chars
        for (app_offset, app_id) in &app_ids {
            let distance = if key_offset > *app_offset {
                key_offset - app_offset
            } else {
                app_offset - key_offset
            };

            if distance < 500 {
                // Look for index name nearby
                if let Some(index_name) = find_index_name_nearby(html, key_offset) {
                    results.push(CandidateCredentials {
                        app_id: app_id.clone(),
                        api_key: key_str.to_string(),
                        index_name,
                    });
                }
                break;
            }
        }
    }

    // Modern keys with context scoring
    for key_match in patterns::MODERN_KEY_RE.find_iter(html) {
        let key_str = key_match.as_str();

        // Skip if it's already a legacy key match or doesn't have mixed content
        if patterns::LEGACY_KEY_RE.is_match(key_str) || !patterns::is_mixed_alphanumeric(key_str) {
            continue;
        }

        let score = patterns::context_score(html, key_match.start(), key_str.len());
        if score >= patterns::MIN_CONFIDENCE {
            for (app_offset, app_id) in &app_ids {
                let distance = if key_match.start() > *app_offset {
                    key_match.start() - app_offset
                } else {
                    app_offset - key_match.start()
                };

                if distance < 500 {
                    if let Some(index_name) = find_index_name_nearby(html, key_match.start()) {
                        results.push(CandidateCredentials {
                            app_id: app_id.clone(),
                            api_key: key_str.to_string(),
                            index_name,
                        });
                    }
                    break;
                }
            }
        }
    }

    results
}

// Helper: extract credentials from a key-value map
fn credentials_from_kv(kv: &std::collections::HashMap<String, String>) -> Option<CandidateCredentials> {
    let app_id = find_value(kv, APP_ID_PROPERTIES)?;
    let api_key = find_value(kv, API_KEY_PROPERTIES)?;
    let index_name = find_value(kv, INDEX_NAME_PROPERTIES)?;

    if app_id.is_empty() || api_key.is_empty() || index_name.is_empty() {
        return None;
    }

    Some(CandidateCredentials {
        app_id,
        api_key,
        index_name,
    })
}

fn find_value(kv: &std::collections::HashMap<String, String>, names: &[&str]) -> Option<String> {
    for name in names {
        if let Some(v) = kv.get(*name) {
            return Some(v.clone());
        }
    }
    None
}

// Helper: find an index name near a given offset
static INDEX_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(?:\w*index[-_]?name\w*)\s*[:='"]\s*['"`]?([a-zA-Z0-9_-]+)['"`]?"#,
    )
    .unwrap()
});

fn find_index_name_nearby(text: &str, offset: usize) -> Option<String> {
    let window_start = offset.saturating_sub(1000);
    let window_end = (offset + 1000).min(text.len());
    let window = &text[window_start..window_end];

    INDEX_NAME_RE
        .captures(window)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

// Helper: recursively search JSON for algolia config
fn extract_algolia_from_json(value: &serde_json::Value) -> Option<CandidateCredentials> {
    match value {
        serde_json::Value::Object(map) => {
            // Check if this object has appId + apiKey + indexName directly
            let app_id = map
                .iter()
                .find(|(k, _)| APP_ID_PROPERTIES.contains(&k.as_str()))
                .and_then(|(_, v)| v.as_str())
                .map(|s| s.to_string());
            let api_key = map
                .iter()
                .find(|(k, _)| API_KEY_PROPERTIES.contains(&k.as_str()))
                .and_then(|(_, v)| v.as_str())
                .map(|s| s.to_string());
            let index_name = map
                .iter()
                .find(|(k, _)| INDEX_NAME_PROPERTIES.contains(&k.as_str()))
                .and_then(|(_, v)| v.as_str())
                .map(|s| s.to_string());

            if let (Some(a), Some(k), Some(i)) = (app_id, api_key, index_name) {
                return Some(CandidateCredentials {
                    app_id: a,
                    api_key: k,
                    index_name: i,
                });
            }

            // Recurse into nested objects
            for (_, v) in map {
                if let Some(cred) = extract_algolia_from_json(v) {
                    return Some(cred);
                }
            }

            None
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                if let Some(cred) = extract_algolia_from_json(item) {
                    return Some(cred);
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preconnect_quoted() {
        let html = r#"<link rel="preconnect" href="https://X1Z85QJPUV-dsn.algolia.net" crossorigin="anonymous" />"#;
        let caps: Vec<_> = PRECONNECT_RE.captures_iter(html).collect();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].get(1).unwrap().as_str(), "X1Z85QJPUV");
    }

    #[test]
    fn test_preconnect_unquoted() {
        let html = r#"<link rel=preconnect href=https://X1Z85QJPUV-dsn.algolia.net crossorigin=anonymous />"#;
        let caps: Vec<_> = PRECONNECT_RE.captures_iter(html).collect();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].get(1).unwrap().as_str(), "X1Z85QJPUV");
    }

    #[test]
    fn test_preconnect_algolianet() {
        let html = r#"<link rel="preconnect" href="https://BH4D9OD16A-dsn.algolianet.com">"#;
        // This should NOT match — algolianet.com uses a different pattern
        let caps: Vec<_> = PRECONNECT_RE.captures_iter(html).collect();
        assert_eq!(caps.len(), 0);
    }

    #[test]
    fn test_docsearch_init_extraction() {
        let html = r#"
            <script>
                docsearch({
                    appId: 'BH4D9OD16A',
                    apiKey: 'd9aa2d7a17b51cc4b053e1ee0bd1d4b5',
                    indexName: 'my-docs',
                });
            </script>
        "#;
        let results = scan_docsearch_init(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].app_id, "BH4D9OD16A");
        assert_eq!(results[0].api_key, "d9aa2d7a17b51cc4b053e1ee0bd1d4b5");
        assert_eq!(results[0].index_name, "my-docs");
    }

    #[test]
    fn test_meta_tag_extraction() {
        let html = r#"
            <html><head>
                <meta name="docsearch:appId" content="BH4D9OD16A">
                <meta name="docsearch:apiKey" content="d9aa2d7a17b51cc4b053e1ee0bd1d4b5">
                <meta name="docsearch:indexName" content="my-docs">
            </head><body></body></html>
        "#;
        let results = scan_meta_tags(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].app_id, "BH4D9OD16A");
        assert_eq!(results[0].index_name, "my-docs");
    }

    #[test]
    fn test_data_docsearch_attributes() {
        let html = r#"
            <html><head></head>
            <body>
                <div data-docsearch-app-id="BH4D9OD16A"
                     data-docsearch-api-key="d9aa2d7a17b51cc4b053e1ee0bd1d4b5"
                     data-docsearch-index-name="my-docs">
                </div>
            </body></html>
        "#;
        let results = scan_meta_tags(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].app_id, "BH4D9OD16A");
    }

    #[test]
    fn test_next_data_extraction() {
        let html = r#"
            <script id="__NEXT_DATA__" type="application/json">
            {"props":{"pageProps":{"algolia":{"appId":"BH4D9OD16A","apiKey":"d9aa2d7a17b51cc4b053e1ee0bd1d4b5","indexName":"my-docs"}}}}
            </script>
        "#;
        let results = scan_framework_hydration(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].app_id, "BH4D9OD16A");
        assert_eq!(results[0].index_name, "my-docs");
    }

    #[test]
    fn test_generic_config_object() {
        let html = r#"
            <script>
                const config = {
                    algolia: {
                        appId: 'BH4D9OD16A',
                        apiKey: 'd9aa2d7a17b51cc4b053e1ee0bd1d4b5',
                        indexName: 'my-docs',
                    }
                };
            </script>
        "#;
        let results = scan_generic_config_objects(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].app_id, "BH4D9OD16A");
    }

    #[test]
    fn test_scan_html_deduplicates_across_strategies() {
        // Meta tags + data attributes in same page should produce one result
        let html = r#"
            <html><head>
                <meta name="docsearch:appId" content="BH4D9OD16A">
                <meta name="docsearch:apiKey" content="d9aa2d7a17b51cc4b053e1ee0bd1d4b5">
                <meta name="docsearch:indexName" content="my-docs">
            </head>
            <body>
                <div data-docsearch-app-id="BH4D9OD16A"
                     data-docsearch-api-key="d9aa2d7a17b51cc4b053e1ee0bd1d4b5"
                     data-docsearch-index-name="my-docs">
                </div>
            </body></html>
        "#;
        let results = scan_html(html);
        // Both strategies find the same credentials — dedup happens at orchestrator level
        // But scan_html itself returns all candidates (dedup is in mod.rs)
        assert!(results.len() >= 1);
        // All results should have the same credentials
        for r in &results {
            assert_eq!(r.app_id, "BH4D9OD16A");
        }
    }

    #[test]
    fn test_preconnect_with_nearby_credentials() {
        let html = r#"
            <link rel=preconnect href=https://X1Z85QJPUV-dsn.algolia.net crossorigin=anonymous />
            <script>
                var config = {
                    appId: "X1Z85QJPUV",
                    apiKey: "1be39e4a1d73c3b1e1082e3c1c5b263d",
                    indexName: "docusaurus-2"
                };
            </script>
        "#;
        let results = scan_preconnect_hints(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].app_id, "X1Z85QJPUV");
        assert_eq!(results[0].api_key, "1be39e4a1d73c3b1e1082e3c1c5b263d");
        assert_eq!(results[0].index_name, "docusaurus-2");
    }

    #[test]
    fn test_escaped_json_rsc_payload() {
        // Simulates React Server Components escaped JSON payload (redcare.it pattern)
        let html = r#"something\"algoliaConfig\":{\"algoliaApiKey\":\"6706777b1652b0b3d519958312d1ffa1\",\"algoliaApplicationId\":\"58ECUELY50\",\"productsAlgoliaIndex\":\"products_prod\"}\"more stuff"#;
        let results = scan_escaped_json(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].app_id, "58ECUELY50");
        assert_eq!(results[0].api_key, "6706777b1652b0b3d519958312d1ffa1");
        assert_eq!(results[0].index_name, "products_prod");
    }

    #[test]
    fn test_escaped_json_no_match_without_algolia() {
        let html = r#"someConfig\":{\"key\":\"value\"}"#;
        let results = scan_escaped_json(html);
        assert!(results.is_empty());
    }

    #[test]
    fn test_window_globals_env() {
        let html = r#"
            <script>
                window.env = {"ALGOLIA_APPLICATION_ID":"HW3T8WVS73","ALGOLIA_SEARCH_API_KEY":"a44069a5116559934332f93aa82d91d8","ALGOLIA_INDEX_NAME":"products"}
            </script>
        "#;
        let results = scan_window_globals(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].app_id, "HW3T8WVS73");
        assert_eq!(results[0].api_key, "a44069a5116559934332f93aa82d91d8");
        assert_eq!(results[0].index_name, "products");
    }

    #[test]
    fn test_window_globals_partial_no_index() {
        // When index_name is missing, should still extract app_id + api_key
        let html = r#"
            <script>
                window.env = {"ALGOLIA_APPLICATION_ID":"HW3T8WVS73","ALGOLIA_SEARCH_API_KEY":"a44069a5116559934332f93aa82d91d8","OTHER_KEY":"value"}
            </script>
        "#;
        let results = scan_window_globals(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].app_id, "HW3T8WVS73");
        assert_eq!(results[0].api_key, "a44069a5116559934332f93aa82d91d8");
        assert!(results[0].index_name.is_empty());
    }

    #[test]
    fn test_window_globals_config() {
        let html = r#"
            <script>
                window.config = {"appId":"BH4D9OD16A","apiKey":"d9aa2d7a17b51cc4b053e1ee0bd1d4b5","indexName":"my-docs"}
            </script>
        "#;
        let results = scan_window_globals(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].app_id, "BH4D9OD16A");
        assert_eq!(results[0].index_name, "my-docs");
    }

    #[test]
    fn test_window_globals_no_match() {
        let html = r#"<script>window.env = {"API_URL":"https://example.com"}</script>"#;
        let results = scan_window_globals(html);
        assert!(results.is_empty());
    }
}
