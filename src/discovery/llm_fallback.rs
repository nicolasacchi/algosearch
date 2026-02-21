pub fn generate_instructions(url: &url::Url) -> serde_json::Value {
    serde_json::json!({
        "error": "autodiscovery_failed",
        "url": url.to_string(),
        "layers_attempted": ["html", "js_bundles"],
        "fallback_instructions": {
            "method": "browser_automation",
            "description": "Use playwright or similar to discover Algolia credentials dynamically",
            "steps": [
                "Open the URL in a headless browser (playwright/puppeteer)",
                "Wait for full page load including JS execution",
                "Execute in page context: JSON.stringify(window.docsearchConfig || window.__docsearch || null)",
                "If null, execute: document.querySelectorAll('[data-docsearch-app-id], [data-algolia-app-id]').forEach(el => console.log(el.dataset))",
                "If still null, intercept network requests matching *algolia.net* or *algolia.io* — the request headers will contain x-algolia-api-key and x-algolia-application-id",
                "If still null, search all script elements for string patterns matching /[A-Z0-9]{10}/ near /[a-f0-9]{32}/ (algolia appId and apiKey formats)",
                format!("Once found, validate by POSTing to https://{{appId}}-dsn.algolia.net/1/indexes/{{indexName}}/query with headers x-algolia-api-key and x-algolia-application-id, body {{\"query\":\"\",\"hitsPerPage\":1}}")
            ],
            "expected_output": {
                "app_id": "10 char uppercase alphanumeric",
                "api_key": "32+ char alphanumeric",
                "index_name": "string",
                "facet_filters": "optional array of strings"
            },
            "after_discovery": format!("Run: algosearch add {} --credentials <appId>:<apiKey>:<indexName> to register manually", url)
        }
    })
}
