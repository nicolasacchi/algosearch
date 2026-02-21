pub mod extraction;
pub mod html;
pub mod js;
pub mod llm_fallback;
pub mod patterns;
pub mod validate;

use crate::error::{AppError, AppResult};
use crate::registry::AlgoliaIndex;
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CandidateCredentials {
    pub app_id: String,
    pub api_key: String,
    pub index_name: String,
}

#[derive(Debug, Serialize)]
pub struct DiscoveredSite {
    pub url: String,
    pub indices: Vec<AlgoliaIndex>,
    pub discovery_method: String,
}

pub enum DiscoveryResult {
    Found(DiscoveredSite),
    LlmFallback(serde_json::Value),
}

pub async fn discover(
    url: &url::Url,
    client: &reqwest::Client,
    verbose: bool,
) -> AppResult<DiscoveryResult> {
    // Fetch HTML body
    let resp = client
        .get(url.as_str())
        .send()
        .await
        .map_err(|e| AppError::Network(e))?;

    let html_body = resp.text().await.map_err(|e| AppError::Network(e))?;

    // Layer 1: HTML scanning
    if verbose {
        eprintln!("... scanning HTML for credentials");
    }
    let mut candidates: Vec<CandidateCredentials> = html::scan_html(&html_body);

    // Check if any candidates are missing index_name — we'll need JS scanning regardless
    let has_partial = candidates.iter().any(|c| c.index_name.is_empty());

    // Layer 2: JS bundle scanning (if layer 1 found nothing, has partial results, or verbose)
    if candidates.is_empty() || has_partial || verbose {
        if verbose {
            eprintln!("... scanning JS bundles");
        }
        let js_candidates = js::scan_js_bundles(url, &html_body, client).await;
        candidates.extend(js_candidates);
    }

    // Enrich partial candidates: if we have app_id + api_key but no index_name,
    // try to find index names from the full set of discovered candidates or the HTML
    let all_index_names: Vec<String> = candidates
        .iter()
        .filter(|c| !c.index_name.is_empty())
        .map(|c| c.index_name.clone())
        .collect();

    if !all_index_names.is_empty() {
        // Clone partial candidates and fill in index names from other candidates
        let mut enriched = Vec::new();
        for cred in &candidates {
            if cred.index_name.is_empty() {
                for idx_name in &all_index_names {
                    enriched.push(CandidateCredentials {
                        app_id: cred.app_id.clone(),
                        api_key: cred.api_key.clone(),
                        index_name: idx_name.clone(),
                    });
                }
            }
        }
        candidates.extend(enriched);
    } else {
        // No index names found from other candidates — search HTML for any index name patterns
        let mut found_names = extract_all_index_names(&html_body);

        // Also fetch algolia-related JS files to find index names
        if found_names.is_empty() && candidates.iter().any(|c| c.index_name.is_empty()) {
            if verbose {
                eprintln!("... searching JS bundles for index names");
            }
            let js_names = find_index_names_in_js(url, &html_body, client).await;
            found_names.extend(js_names);
        }

        if !found_names.is_empty() {
            let mut enriched = Vec::new();
            for cred in &candidates {
                if cred.index_name.is_empty() {
                    for idx_name in &found_names {
                        enriched.push(CandidateCredentials {
                            app_id: cred.app_id.clone(),
                            api_key: cred.api_key.clone(),
                            index_name: idx_name.clone(),
                        });
                    }
                }
            }
            candidates.extend(enriched);
        }
    }

    // Remove candidates with empty index_name (they couldn't be enriched)
    candidates.retain(|c| !c.index_name.is_empty());

    // Deduplicate
    let unique: HashSet<CandidateCredentials> = candidates.into_iter().collect();
    let candidates: Vec<CandidateCredentials> = unique.into_iter().collect();

    if candidates.is_empty() {
        // Layer 3: LLM fallback instructions
        let fallback = llm_fallback::generate_instructions(url);
        return Ok(DiscoveryResult::LlmFallback(fallback));
    }

    // Validate candidates
    if verbose {
        eprintln!("... validating {} candidate(s)", candidates.len());
    }
    let validated = validate::validate_candidates(&candidates, client).await?;

    if validated.is_empty() {
        return Err(AppError::DiscoveryFailed {
            url: url.to_string(),
            suggestion: Some("all extracted credentials failed validation — the site may not use Algolia DocSearch".to_string()),
        });
    }

    Ok(DiscoveryResult::Found(DiscoveredSite {
        url: url.to_string(),
        indices: validated,
        discovery_method: "auto".to_string(),
    }))
}

/// Extract all potential index names from text using broad patterns.
static ALL_INDEX_NAMES_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:\w*index[-_]?name\w*)\s*[:=]\s*['"`]([a-zA-Z0-9_-]+)['"`]"#).unwrap()
});

fn extract_all_index_names(text: &str) -> Vec<String> {
    let mut names: Vec<String> = ALL_INDEX_NAMES_RE
        .captures_iter(text)
        .filter_map(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Fetch algolia-related JS files and search them for index names.
static JS_SRC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<script[^>]+src=(?:["']([^"']+)["']|([^\s>]+))"#).unwrap()
});

async fn find_index_names_in_js(
    base_url: &url::Url,
    html: &str,
    client: &reqwest::Client,
) -> Vec<String> {
    // Find JS URLs that are likely algolia-related
    let mut js_urls: Vec<String> = Vec::new();
    for cap in JS_SRC_RE.captures_iter(html) {
        let src = cap.get(1).or_else(|| cap.get(2));
        if let Some(src) = src {
            let src_str = src.as_str().to_lowercase();
            if src_str.contains("algolia") || src_str.contains("search") {
                if let Ok(resolved) = base_url.join(src.as_str()) {
                    js_urls.push(resolved.to_string());
                }
            }
        }
    }

    // Also check importmap entries for algolia-related JS
    static IMPORTMAP_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#""[^"]*algolia[^"]*"\s*:\s*"([^"]+)""#).unwrap()
    });
    for cap in IMPORTMAP_RE.captures_iter(html) {
        if let Some(url_match) = cap.get(1) {
            if let Ok(resolved) = base_url.join(url_match.as_str()) {
                js_urls.push(resolved.to_string());
            }
        }
    }

    js_urls.truncate(5);

    let mut all_names = Vec::new();
    for url in js_urls {
        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(body) = resp.text().await {
                if body.len() <= 2 * 1024 * 1024 {
                    all_names.extend(extract_all_index_names(&body));
                }
            }
        }
    }

    all_names.sort();
    all_names.dedup();
    all_names
}
