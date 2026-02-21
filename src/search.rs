use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};

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
    pub hierarchy: Vec<String>,
    pub snippet: Option<String>,
    pub url: Option<String>,
    pub hit_type: Option<String>,
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
        "attributesToRetrieve": [
            "hierarchy.lvl0", "hierarchy.lvl1", "hierarchy.lvl2",
            "hierarchy.lvl3", "hierarchy.lvl4", "hierarchy.lvl5",
            "content", "url", "anchor", "type"
        ],
        "attributesToSnippet": ["content:40"],
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
pub struct AlgoliaRawResponse {
    pub hits: Vec<AlgoliaHit>,
    #[serde(rename = "nbHits")]
    pub nb_hits: u64,
}

#[derive(Debug, Deserialize)]
pub struct AlgoliaHit {
    pub hierarchy: Option<HitHierarchy>,
    pub content: Option<String>,
    pub url: Option<String>,
    pub anchor: Option<String>,
    #[serde(rename = "type")]
    pub hit_type: Option<String>,
    #[serde(rename = "_snippetResult")]
    pub snippet_result: Option<SnippetResult>,
}

#[derive(Debug, Deserialize)]
pub struct HitHierarchy {
    pub lvl0: Option<String>,
    pub lvl1: Option<String>,
    pub lvl2: Option<String>,
    pub lvl3: Option<String>,
    pub lvl4: Option<String>,
    pub lvl5: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SnippetResult {
    pub content: Option<SnippetValue>,
}

#[derive(Debug, Deserialize)]
pub struct SnippetValue {
    pub value: Option<String>,
}

impl AlgoliaHit {
    pub fn to_search_hit(&self) -> SearchHit {
        let hierarchy: Vec<String> = if let Some(h) = &self.hierarchy {
            [&h.lvl0, &h.lvl1, &h.lvl2, &h.lvl3, &h.lvl4, &h.lvl5]
                .iter()
                .filter_map(|lvl| lvl.as_ref())
                .filter(|s| !s.is_empty())
                .cloned()
                .collect()
        } else {
            vec![]
        };

        let title = hierarchy.last().cloned();

        let snippet = self
            .snippet_result
            .as_ref()
            .and_then(|sr| sr.content.as_ref())
            .and_then(|c| c.value.as_ref())
            .cloned()
            .or_else(|| self.content.clone());

        let url = match (&self.url, &self.anchor) {
            (Some(u), Some(a)) if !a.is_empty() => Some(format!("{}#{}", u, a)),
            (Some(u), _) => Some(u.clone()),
            _ => None,
        };

        SearchHit {
            title,
            hierarchy,
            snippet,
            url,
            hit_type: self.hit_type.clone(),
        }
    }
}
