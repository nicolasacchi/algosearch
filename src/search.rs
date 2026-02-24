use crate::error::{AppError, AppResult};
use crate::schema::{self, FieldMapping};
use serde::{Deserialize, Serialize};
use urlencoding;

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub site: String,
    pub query: String,
    pub total_hits: u64,
    pub results: Vec<SearchHit>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchHit {
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hierarchy: Vec<String>,
    pub snippet: Option<String>,
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// All raw fields from the Algolia hit (included in JSON output only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
}

pub async fn search_algolia(
    client: &reqwest::Client,
    app_id: &str,
    api_key: &str,
    index_name: &str,
    query: &str,
    filters: &[(String, String)],
    hits_per_page: u32,
) -> AppResult<AlgoliaRawResponse> {
    let url = format!(
        "https://{}-dsn.algolia.net/1/indexes/{}/query",
        app_id, index_name
    );

    let facet_filters: Vec<String> = filters
        .iter()
        .map(|(k, v)| format!("{}:{}", k, v))
        .collect();

    let mut body = serde_json::json!({
        "query": query,
        "hitsPerPage": hits_per_page,
        "attributesToRetrieve": ["*"],
        "attributesToSnippet": ["*:40"],
    });

    if !facet_filters.is_empty() {
        body["facetFilters"] = serde_json::json!(facet_filters);
    }

    let resp = client
        .post(&url)
        .header("x-algolia-application-id", app_id)
        .header("x-algolia-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    if status == 403 {
        return Err(AppError::AlgoliaApi {
            status: 403,
            message: "forbidden — API key may have expired or rotated".to_string(),
            suggestion: Some("try: algosearch refresh <site>".to_string()),
        });
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::AlgoliaApi {
            status: status.as_u16(),
            message: text,
            suggestion: None,
        });
    }

    let raw: AlgoliaRawResponse = resp.json().await?;
    Ok(raw)
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct AlgoliaRawResponse {
    pub hits: Vec<serde_json::Value>,
    #[serde(rename = "nbHits")]
    pub nb_hits: u64,
    #[serde(default)]
    pub facets: Option<serde_json::Value>,
    #[serde(rename = "nbPages", default)]
    pub nb_pages: Option<u64>,
    pub page: Option<u64>,
}

/// Response from the multi-query endpoint `/1/indexes/*/queries`.
#[derive(Debug, Deserialize)]
pub struct AlgoliaMultiQueryResponse {
    pub results: Vec<AlgoliaRawResponse>,
}

/// A single query within a multi-query batch.
#[derive(Debug, Serialize, Clone)]
pub struct MultiQueryRequest {
    #[serde(rename = "indexName")]
    pub index_name: String,
    pub params: String,
}

/// Search with pagination support.
pub async fn search_algolia_paged(
    client: &reqwest::Client,
    app_id: &str,
    api_key: &str,
    index_name: &str,
    query: &str,
    filters: &[(String, String)],
    hits_per_page: u32,
    page: u32,
) -> AppResult<AlgoliaRawResponse> {
    let url = format!(
        "https://{}-dsn.algolia.net/1/indexes/{}/query",
        app_id, index_name
    );

    let facet_filters: Vec<String> = filters
        .iter()
        .map(|(k, v)| format!("{}:{}", k, v))
        .collect();

    let mut body = serde_json::json!({
        "query": query,
        "hitsPerPage": hits_per_page,
        "page": page,
        "attributesToRetrieve": ["*"],
        "attributesToSnippet": ["*:40"],
    });

    if !facet_filters.is_empty() {
        body["facetFilters"] = serde_json::json!(facet_filters);
    }

    let resp = client
        .post(&url)
        .header("x-algolia-application-id", app_id)
        .header("x-algolia-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    if status == 403 {
        return Err(AppError::AlgoliaApi {
            status: 403,
            message: "forbidden — API key may have expired or rotated".to_string(),
            suggestion: Some("try: algosearch refresh <site>".to_string()),
        });
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::AlgoliaApi {
            status: status.as_u16(),
            message: text,
            suggestion: None,
        });
    }

    let raw: AlgoliaRawResponse = resp.json().await?;
    Ok(raw)
}

/// Fetch facet values for a given attribute.
pub async fn fetch_facets(
    client: &reqwest::Client,
    app_id: &str,
    api_key: &str,
    index_name: &str,
    facet_attribute: &str,
    max_values: u32,
) -> AppResult<Vec<(String, u64)>> {
    let url = format!(
        "https://{}-dsn.algolia.net/1/indexes/{}/query",
        app_id, index_name
    );

    let body = serde_json::json!({
        "query": "",
        "hitsPerPage": 0,
        "facets": [facet_attribute],
        "maxValuesPerFacet": max_values,
        "analytics": false,
    });

    let resp = client
        .post(&url)
        .header("x-algolia-application-id", app_id)
        .header("x-algolia-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::AlgoliaApi {
            status: status.as_u16(),
            message: text,
            suggestion: None,
        });
    }

    let data: serde_json::Value = resp.json().await?;
    let mut facet_values: Vec<(String, u64)> = Vec::new();

    if let Some(facets) = data.get("facets").and_then(|f| f.get(facet_attribute)).and_then(|v| v.as_object()) {
        for (name, count) in facets {
            let c = count.as_u64().unwrap_or(0);
            facet_values.push((name.clone(), c));
        }
    }

    // Sort by count descending
    facet_values.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(facet_values)
}

/// Execute a batch of queries in a single API call using the multi-query endpoint.
pub async fn multi_query(
    client: &reqwest::Client,
    app_id: &str,
    api_key: &str,
    queries: &[MultiQueryRequest],
) -> AppResult<AlgoliaMultiQueryResponse> {
    let url = format!(
        "https://{}-dsn.algolia.net/1/indexes/*/queries",
        app_id
    );

    let body = serde_json::json!({
        "requests": queries,
    });

    let resp = client
        .post(&url)
        .header("x-algolia-application-id", app_id)
        .header("x-algolia-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::AlgoliaApi {
            status: status.as_u16(),
            message: text,
            suggestion: None,
        });
    }

    let raw: AlgoliaMultiQueryResponse = resp.json().await?;
    Ok(raw)
}

/// Build multi-query params string from components.
pub fn build_query_params(
    query: &str,
    facet_filters: &[String],
    hits_per_page: u32,
    page: u32,
    attributes: Option<&[&str]>,
) -> String {
    let mut params = vec![
        format!("query={}", urlencoding::encode(query)),
        format!("hitsPerPage={}", hits_per_page),
        format!("page={}", page),
    ];

    if let Some(attrs) = attributes {
        let attr_json = serde_json::json!(attrs);
        params.push(format!("attributesToRetrieve={}", urlencoding::encode(&attr_json.to_string())));
    } else {
        params.push("attributesToRetrieve=%5B%22*%22%5D".to_string()); // ["*"]
    }

    if !facet_filters.is_empty() {
        let ff_json = serde_json::json!(facet_filters);
        params.push(format!("facetFilters={}", urlencoding::encode(&ff_json.to_string())));
    }

    params.join("&")
}

/// Convert a raw Algolia hit into a SearchHit using the field mapping.
/// Falls back to DocSearch defaults when no mapping is provided.
pub fn convert_hit(
    hit: &serde_json::Value,
    mapping: Option<&FieldMapping>,
    include_raw: bool,
) -> SearchHit {
    let default = schema::default_docsearch_mapping();
    let m = mapping.unwrap_or(&default);

    // Extract hierarchy breadcrumbs (DocSearch-style)
    let hierarchy: Vec<String> = m
        .hierarchy
        .iter()
        .filter_map(|path| schema::extract_string(hit, path))
        .filter(|s| !s.is_empty())
        .collect();

    // Title: from mapping field, or deepest hierarchy level
    let title = if let Some(title_path) = &m.title {
        schema::extract_string(hit, title_path)
    } else {
        hierarchy.last().cloned()
    };

    // Snippet: try _snippetResult for the description field first, then raw field
    let snippet = extract_snippet(hit, m);

    // URL: base + optional anchor fragment
    let url = build_url(hit, m);

    // Optional fields — for price, prefer pre-formatted strings over raw numbers
    let price = m.price.as_ref().and_then(|p| {
        // Try formatted variant first (e.g. "priceFormatted" or "price.formatted")
        let formatted_key = format!("{}Formatted", p);
        let dotted_key = format!("{}.formatted", p);
        schema::extract_string(hit, &formatted_key)
            .or_else(|| schema::extract_string(hit, &dotted_key))
            .or_else(|| schema::extract_string(hit, p))
    });
    let image = m
        .image
        .as_ref()
        .and_then(|p| schema::extract_string(hit, p));
    let brand = m
        .brand
        .as_ref()
        .and_then(|p| schema::extract_string(hit, p));
    let category = m
        .category
        .as_ref()
        .and_then(|p| schema::extract_string(hit, p));
    let hit_type = schema::extract_string(hit, "type");

    // Strip raw of internal Algolia fields for cleaner output
    let raw = if include_raw {
        let mut cleaned = hit.clone();
        if let Some(obj) = cleaned.as_object_mut() {
            obj.retain(|k, _| !k.starts_with('_') && k != "objectID");
        }
        Some(cleaned)
    } else {
        None
    };

    SearchHit {
        title,
        hierarchy,
        snippet,
        url,
        hit_type,
        price,
        image,
        brand,
        category,
        raw,
    }
}

/// Try to extract a snippet from the Algolia _snippetResult, falling back to the raw field.
fn extract_snippet(hit: &serde_json::Value, m: &FieldMapping) -> Option<String> {
    if let Some(desc_field) = &m.description {
        // Try _snippetResult.<field>.value first (Algolia's snippet format)
        if let Some(snippet_val) = hit
            .get("_snippetResult")
            .and_then(|sr| sr.get(desc_field.as_str()))
            .and_then(|c| c.get("value"))
            .and_then(|v| v.as_str())
        {
            return Some(snippet_val.to_string());
        }
        // Fall back to raw field value
        if let Some(s) = schema::extract_string(hit, desc_field) {
            // Truncate long descriptions for display
            if s.len() > 200 {
                return Some(format!("{}...", &s[..200]));
            }
            return Some(s);
        }
    }
    None
}

/// Build a URL from the mapping, optionally appending an anchor fragment.
fn build_url(hit: &serde_json::Value, m: &FieldMapping) -> Option<String> {
    let base = m
        .url
        .as_ref()
        .and_then(|p| schema::extract_string(hit, p))?;
    if let Some(anchor_path) = &m.url_anchor {
        if let Some(anchor) = schema::extract_string(hit, anchor_path) {
            if !anchor.is_empty() {
                return Some(format!("{}#{}", base, anchor));
            }
        }
    }
    Some(base)
}
