use crate::discovery::html::scan_html;
use crate::discovery::CandidateCredentials;
use regex::Regex;
use std::sync::LazyLock;

const MAX_JS_FILES: usize = 20;
const MAX_CONCURRENT: usize = 5;
const MAX_FILE_SIZE: usize = 2 * 1024 * 1024; // 2MB

// Matches <script src="..."> and <script src=...> (quoted or unquoted)
static SCRIPT_SRC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<script[^>]+src=(?:["']([^"']+)["']|([^\s>]+))"#).unwrap()
});

// Matches <link rel="modulepreload" href="..."> and unquoted variants
static MODULEPRELOAD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<link[^>]+rel=(?:["']modulepreload["']|modulepreload)[^>]+href=(?:["']([^"']+)["']|([^\s>]+))"#).unwrap()
});

/// Keywords that suggest a JS file may contain search config.
const JS_KEYWORDS: &[&str] = &[
    "search", "docsearch", "algolia", "config", "app", "main", "index", "chunk",
];

pub async fn scan_js_bundles(
    base_url: &url::Url,
    html: &str,
    client: &reqwest::Client,
) -> Vec<CandidateCredentials> {
    let mut js_urls: Vec<String> = Vec::new();

    // Extract script sources (group 1 = quoted, group 2 = unquoted)
    for cap in SCRIPT_SRC_RE.captures_iter(html) {
        let src = cap.get(1).or_else(|| cap.get(2));
        if let Some(src) = src {
            js_urls.push(src.as_str().to_string());
        }
    }

    // Extract modulepreload hrefs (group 1 = quoted, group 2 = unquoted)
    for cap in MODULEPRELOAD_RE.captures_iter(html) {
        let href = cap.get(1).or_else(|| cap.get(2));
        if let Some(href) = href {
            js_urls.push(href.as_str().to_string());
        }
    }

    // Resolve relative URLs and filter to likely candidates
    let mut resolved_urls: Vec<String> = js_urls
        .into_iter()
        .filter_map(|src| {
            base_url.join(&src).ok().map(|u| u.to_string())
        })
        .filter(|url| is_likely_candidate(url))
        .collect();

    resolved_urls.truncate(MAX_JS_FILES);

    // Fetch concurrently with bounded concurrency
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT));
    let mut handles = Vec::new();

    for url in resolved_urls {
        let permit = semaphore.clone();
        let client = client.clone();
        let handle = tokio::spawn(async move {
            let _permit = permit.acquire().await;
            fetch_and_extract(&client, &url).await
        });
        handles.push(handle);
    }

    let mut all_candidates = Vec::new();
    for handle in handles {
        if let Ok(candidates) = handle.await {
            all_candidates.extend(candidates);
        }
    }

    all_candidates
}

fn is_likely_candidate(url: &str) -> bool {
    let lower = url.to_lowercase();

    // Always include hashed chunk files (common in webpack/vite output)
    if lower.contains("chunk") || lower.contains("[hash]") {
        return true;
    }

    // Include files with content hashes in the name (e.g. main.e332b48e.js)
    if HASHED_FILENAME_RE.is_match(&lower) {
        return true;
    }

    // Check for keyword matches
    JS_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

static HASHED_FILENAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\.[a-f0-9]{6,12}\.js$").unwrap()
});

async fn fetch_and_extract(client: &reqwest::Client, url: &str) -> Vec<CandidateCredentials> {
    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    // Check content-length header before downloading
    if let Some(len) = resp.content_length() {
        if len as usize > MAX_FILE_SIZE {
            return vec![];
        }
    }

    let body = match resp.text().await {
        Ok(b) if b.len() <= MAX_FILE_SIZE => b,
        _ => return vec![],
    };

    // Reuse the HTML scanner — it works on any text containing JS
    scan_html(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_src_quoted() {
        let html = r#"<script src="/assets/js/main.abc123.js" defer></script>"#;
        let caps: Vec<_> = SCRIPT_SRC_RE.captures_iter(html).collect();
        assert_eq!(caps.len(), 1);
        let src = caps[0].get(1).or_else(|| caps[0].get(2)).unwrap();
        assert_eq!(src.as_str(), "/assets/js/main.abc123.js");
    }

    #[test]
    fn test_script_src_unquoted() {
        let html = r#"<script src=/assets/js/main.e332b48e.js defer></script>"#;
        let caps: Vec<_> = SCRIPT_SRC_RE.captures_iter(html).collect();
        assert_eq!(caps.len(), 1);
        let src = caps[0].get(1).or_else(|| caps[0].get(2)).unwrap();
        assert_eq!(src.as_str(), "/assets/js/main.e332b48e.js");
    }

    #[test]
    fn test_script_src_mixed() {
        let html = concat!(
            r#"<script src="/js/quoted.js"></script>"#,
            r#"<script src=/js/unquoted.js defer></script>"#,
            r#"<script src='/js/single.js'></script>"#,
        );
        let caps: Vec<_> = SCRIPT_SRC_RE.captures_iter(html).collect();
        assert_eq!(caps.len(), 3);

        let urls: Vec<&str> = caps
            .iter()
            .map(|c| c.get(1).or_else(|| c.get(2)).unwrap().as_str())
            .collect();
        assert_eq!(urls, vec!["/js/quoted.js", "/js/unquoted.js", "/js/single.js"]);
    }

    #[test]
    fn test_modulepreload_unquoted() {
        let html = r#"<link rel=modulepreload href=/assets/chunks/framework.abc123.js>"#;
        let caps: Vec<_> = MODULEPRELOAD_RE.captures_iter(html).collect();
        assert_eq!(caps.len(), 1);
        let href = caps[0].get(1).or_else(|| caps[0].get(2)).unwrap();
        assert_eq!(href.as_str(), "/assets/chunks/framework.abc123.js");
    }

    #[test]
    fn test_modulepreload_quoted() {
        let html = r#"<link rel="modulepreload" href="/assets/chunks/framework.abc123.js">"#;
        let caps: Vec<_> = MODULEPRELOAD_RE.captures_iter(html).collect();
        assert_eq!(caps.len(), 1);
        let href = caps[0].get(1).or_else(|| caps[0].get(2)).unwrap();
        assert_eq!(href.as_str(), "/assets/chunks/framework.abc123.js");
    }

    #[test]
    fn test_hashed_filename_detection() {
        assert!(is_likely_candidate("/assets/js/main.e332b48e.js"));
        assert!(is_likely_candidate("/js/chunk-abc123de.js"));
        assert!(is_likely_candidate("/js/search.js"));
        assert!(!is_likely_candidate("/assets/js/runtime.js"));
        assert!(!is_likely_candidate("/assets/img/logo.png"));
    }
}
