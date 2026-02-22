use crate::discovery::CandidateCredentials;
use crate::error::AppResult;
use crate::registry::AlgoliaIndex;
use crate::schema;
use std::collections::HashMap;

pub async fn validate_candidates(
    candidates: &[CandidateCredentials],
    client: &reqwest::Client,
) -> AppResult<Vec<AlgoliaIndex>> {
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(3));
    let mut handles = Vec::new();

    for cred in candidates {
        // Skip candidates with no app_id or api_key
        if cred.app_id.is_empty() || cred.api_key.is_empty() {
            continue;
        }

        let sem = semaphore.clone();
        let client = client.clone();
        let cred = cred.clone();
        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await;
            if cred.index_name.is_empty() {
                // Try to discover indices via the Algolia API
                discover_and_validate(&client, &cred).await
            } else {
                match validate_single(&client, &cred).await {
                    Some(idx) => vec![idx],
                    None => vec![],
                }
            }
        });
        handles.push(handle);
    }

    let mut validated = Vec::new();
    for handle in handles {
        if let Ok(indices) = handle.await {
            validated.extend(indices);
        }
    }

    Ok(validated)
}

async fn validate_single(
    client: &reqwest::Client,
    cred: &CandidateCredentials,
) -> Option<AlgoliaIndex> {
    let url = format!(
        "https://{}-dsn.algolia.net/1/indexes/{}/query",
        cred.app_id, cred.index_name
    );

    // First query: validate and get record count
    let body = serde_json::json!({
        "query": "",
        "hitsPerPage": 0,
        "analytics": false,
    });

    let resp = client
        .post(&url)
        .header("x-algolia-application-id", &cred.app_id)
        .header("x-algolia-api-key", &cred.api_key)
        .json(&body)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let data: serde_json::Value = resp.json().await.ok()?;
    let record_count = data.get("nbHits").and_then(|v| v.as_u64());

    // Second query: discover facets
    let facet_body = serde_json::json!({
        "query": "",
        "hitsPerPage": 0,
        "facets": ["*"],
    });

    let facets = if let Ok(resp) = client
        .post(&url)
        .header("x-algolia-application-id", &cred.app_id)
        .header("x-algolia-api-key", &cred.api_key)
        .json(&facet_body)
        .send()
        .await
    {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            parse_facets(&data)
        } else {
            None
        }
    } else {
        None
    };

    // Third query: introspect index schema to detect field mapping
    let field_mapping = schema::introspect_index(client, &cred.app_id, &cred.api_key, &cred.index_name)
        .await
        .map(|attrs| schema::detect_mapping(&attrs));

    Some(AlgoliaIndex {
        app_id: cred.app_id.clone(),
        api_key: cred.api_key.clone(),
        index_name: cred.index_name.clone(),
        record_count,
        facets,
        is_default: true,
        field_mapping,
    })
}

/// When we have app_id + api_key but no index_name, try listing indices
/// or probing common index name patterns.
async fn discover_and_validate(
    client: &reqwest::Client,
    cred: &CandidateCredentials,
) -> Vec<AlgoliaIndex> {
    // First try the "list indices" API (may be blocked by search-only keys)
    let list_url = format!(
        "https://{}-dsn.algolia.net/1/indexes",
        cred.app_id
    );

    if let Ok(resp) = client
        .get(&list_url)
        .header("x-algolia-application-id", &cred.app_id)
        .header("x-algolia-api-key", &cred.api_key)
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                if let Some(items) = data.get("items").and_then(|v| v.as_array()) {
                    let mut results = Vec::new();
                    for item in items {
                        if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                            let full_cred = CandidateCredentials {
                                app_id: cred.app_id.clone(),
                                api_key: cred.api_key.clone(),
                                index_name: name.to_string(),
                            };
                            if let Some(idx) = validate_single(client, &full_cred).await {
                                results.push(idx);
                            }
                            if results.len() >= 5 {
                                break;
                            }
                        }
                    }
                    if !results.is_empty() {
                        return results;
                    }
                }
            }
        }
    }

    // Fallback: try the wildcard search endpoint to discover an index
    // Use the multi-index search API with a "*" query
    let multi_url = format!(
        "https://{}-dsn.algolia.net/1/indexes/*/queries",
        cred.app_id
    );

    let body = serde_json::json!({
        "requests": [{"indexName": "*", "params": "query=&hitsPerPage=0"}]
    });

    if let Ok(resp) = client
        .post(&multi_url)
        .header("x-algolia-application-id", &cred.app_id)
        .header("x-algolia-api-key", &cred.api_key)
        .json(&body)
        .send()
        .await
    {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            if let Some(results_arr) = data.get("results").and_then(|v| v.as_array()) {
                let mut results = Vec::new();
                for result in results_arr {
                    if let Some(index) = result.get("index").and_then(|v| v.as_str()) {
                        let full_cred = CandidateCredentials {
                            app_id: cred.app_id.clone(),
                            api_key: cred.api_key.clone(),
                            index_name: index.to_string(),
                        };
                        if let Some(idx) = validate_single(client, &full_cred).await {
                            results.push(idx);
                        }
                    }
                }
                if !results.is_empty() {
                    return results;
                }
            }
        }
    }

    vec![]
}

fn parse_facets(data: &serde_json::Value) -> Option<HashMap<String, Vec<String>>> {
    let facets_obj = data.get("facets")?.as_object()?;
    let mut result = HashMap::new();

    for (facet_name, values) in facets_obj {
        if let Some(values_obj) = values.as_object() {
            let keys: Vec<String> = values_obj.keys().cloned().collect();
            if !keys.is_empty() {
                result.insert(facet_name.clone(), keys);
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}
