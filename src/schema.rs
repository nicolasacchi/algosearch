use serde::{Deserialize, Serialize};

/// Maps discovered index attributes to semantic display roles.
/// Stored per-index in the registry so search/display can adapt
/// to any Algolia index schema (DocSearch, e-commerce, generic).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FieldMapping {
    /// Attribute path for the display title (e.g. "display_name", "name")
    pub title: Option<String>,
    /// Attribute path for description/snippet (e.g. "short_description", "content")
    pub description: Option<String>,
    /// Attribute path for the URL (e.g. "url", "link")
    pub url: Option<String>,
    /// Anchor field appended to URL as fragment (DocSearch-specific)
    pub url_anchor: Option<String>,
    /// Attribute path for price
    pub price: Option<String>,
    /// Attribute path for image URL
    pub image: Option<String>,
    /// Attribute path for brand/manufacturer
    pub brand: Option<String>,
    /// Attribute path for category
    pub category: Option<String>,
    /// Ordered hierarchy fields for breadcrumb display (DocSearch-style)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hierarchy: Vec<String>,
    /// Detected schema profile: "docsearch", "ecommerce", or "generic"
    pub profile: String,
    /// All attribute paths found in the index
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discovered_attributes: Vec<String>,
}

/// Query the index with `attributesToRetrieve: ["*"]` and `hitsPerPage: 1`
/// to discover what fields exist. Returns all attribute paths found.
pub async fn introspect_index(
    client: &reqwest::Client,
    app_id: &str,
    api_key: &str,
    index_name: &str,
) -> Option<Vec<String>> {
    let url = format!(
        "https://{}-dsn.algolia.net/1/indexes/{}/query",
        app_id, index_name
    );

    let body = serde_json::json!({
        "query": "",
        "hitsPerPage": 1,
        "attributesToRetrieve": ["*"],
        "analytics": false,
    });

    let resp = client
        .post(&url)
        .header("x-algolia-application-id", app_id)
        .header("x-algolia-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let data: serde_json::Value = resp.json().await.ok()?;
    let hits = data.get("hits")?.as_array()?;
    let first_hit = hits.first()?;

    let mut paths = Vec::new();
    collect_attribute_paths(first_hit, "", &mut paths);

    // Remove Algolia internal fields
    paths.retain(|p| !p.starts_with('_') && p != "objectID");

    paths.sort();
    paths.dedup();
    Some(paths)
}

/// Detect the schema profile and build a FieldMapping from discovered attributes.
pub fn detect_mapping(attributes: &[String]) -> FieldMapping {
    let has_hierarchy = attributes.iter().any(|a| a == "hierarchy.lvl0");
    let has_docsearch_type = attributes.iter().any(|a| a == "type");
    let has_price = attributes
        .iter()
        .any(|a| matches!(a.as_str(), "price" | "sale_price" | "regular_price" | "price_range"));

    if has_hierarchy && has_docsearch_type {
        build_docsearch_mapping(attributes)
    } else if has_price {
        build_ecommerce_mapping(attributes)
    } else {
        build_generic_mapping(attributes)
    }
}

/// Hardcoded DocSearch mapping for backward compat when no stored mapping exists.
pub fn default_docsearch_mapping() -> FieldMapping {
    FieldMapping {
        title: None, // derived from hierarchy at display time
        description: Some("content".to_string()),
        url: Some("url".to_string()),
        url_anchor: Some("anchor".to_string()),
        price: None,
        image: None,
        brand: None,
        category: None,
        hierarchy: vec![
            "hierarchy.lvl0".to_string(),
            "hierarchy.lvl1".to_string(),
            "hierarchy.lvl2".to_string(),
            "hierarchy.lvl3".to_string(),
            "hierarchy.lvl4".to_string(),
            "hierarchy.lvl5".to_string(),
        ],
        profile: "docsearch".to_string(),
        discovered_attributes: vec![],
    }
}

/// Extract a string value from a JSON value using a dotted attribute path.
/// e.g. "hierarchy.lvl0" traverses `obj["hierarchy"]["lvl0"]`.
/// Arrays are handled: if a level is an array, the first string-like element is used.
pub fn extract_string(hit: &serde_json::Value, path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = hit;

    for part in &parts {
        match current {
            serde_json::Value::Object(map) => {
                current = map.get(*part)?;
            }
            _ => return None,
        }
    }

    match current {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Array(arr) => {
            // Return first string element
            arr.iter().find_map(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                _ => None,
            })
        }
        _ => None,
    }
}

// --- private helpers ---

fn build_docsearch_mapping(attributes: &[String]) -> FieldMapping {
    let hierarchy: Vec<String> = (0..=5)
        .map(|i| format!("hierarchy.lvl{}", i))
        .filter(|h| attributes.iter().any(|a| a == h))
        .collect();

    FieldMapping {
        title: None, // derived from hierarchy
        description: first_match(attributes, &["content", "description"]),
        url: first_match(attributes, &["url", "link", "href"]),
        url_anchor: first_match(attributes, &["anchor"]),
        price: None,
        image: None,
        brand: None,
        category: None,
        hierarchy,
        profile: "docsearch".to_string(),
        discovered_attributes: attributes.to_vec(),
    }
}

fn build_ecommerce_mapping(attributes: &[String]) -> FieldMapping {
    FieldMapping {
        title: first_match(
            attributes,
            &[
                "title", "name", "display_name", "product_name", "heading", "label",
                "productName", "displayName", "legal_name", "legalName",
            ],
        ),
        description: first_match(
            attributes,
            &[
                "description", "short_description", "content", "snippet", "summary", "body",
                "descriptionShort", "shortDescription", "long_description", "descriptionLong",
                "functional_subtitle",
            ],
        ),
        url: first_match(
            attributes,
            &[
                "url", "link", "href", "permalink", "slug", "deeplink", "deepLink",
                "product_url", "productUrl", "canonical_url",
            ],
        ),
        url_anchor: None,
        price: first_match(attributes, &["price", "sale_price", "regular_price", "priceFormatted"]),
        image: first_match(
            attributes,
            &[
                "image", "image_url", "thumbnail", "thumbnail_url",
                "url_for_cover_image", "photo", "picture", "img",
                "imageUrl", "thumbnailUrl", "coverImage",
            ],
        ),
        brand: first_match(attributes, &["brand", "brand_name", "manufacturer", "vendor", "brandName", "brandSearch"]),
        category: first_match(attributes, &["category", "categories", "product_type", "collection", "primaryCategory", "secondaryCategories"]),
        hierarchy: vec![],
        profile: "ecommerce".to_string(),
        discovered_attributes: attributes.to_vec(),
    }
}

fn build_generic_mapping(attributes: &[String]) -> FieldMapping {
    FieldMapping {
        title: first_match(
            attributes,
            &[
                "title", "name", "display_name", "heading", "label", "subject",
                "productName", "displayName", "legal_name", "legalName",
            ],
        ),
        description: first_match(
            attributes,
            &[
                "description", "content", "snippet", "summary", "body", "text",
                "descriptionShort", "shortDescription", "short_description",
            ],
        ),
        url: first_match(
            attributes,
            &["url", "link", "href", "permalink", "slug", "deeplink", "deepLink"],
        ),
        url_anchor: first_match(attributes, &["anchor"]),
        price: first_match(attributes, &["price", "sale_price"]),
        image: first_match(
            attributes,
            &["image", "image_url", "thumbnail", "photo", "picture", "imageUrl"],
        ),
        brand: first_match(attributes, &["brand", "brand_name", "manufacturer", "brandName"]),
        category: first_match(attributes, &["category", "categories", "type", "primaryCategory"]),
        hierarchy: vec![],
        profile: "generic".to_string(),
        discovered_attributes: attributes.to_vec(),
    }
}

/// Return the first attribute name from `candidates` that exists in `attributes` (case-insensitive).
fn first_match(attributes: &[String], candidates: &[&str]) -> Option<String> {
    for candidate in candidates {
        if let Some(actual) = attributes
            .iter()
            .find(|a| a.eq_ignore_ascii_case(candidate))
        {
            return Some(actual.clone());
        }
    }
    None
}

/// Recursively walk a JSON value and collect all attribute paths.
/// Only walks into objects, not arrays.
fn collect_attribute_paths(value: &serde_json::Value, prefix: &str, paths: &mut Vec<String>) {
    if let Some(obj) = value.as_object() {
        for (key, val) in obj {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };
            paths.push(path.clone());
            if val.is_object() {
                collect_attribute_paths(val, &path, paths);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_string_simple() {
        let hit = serde_json::json!({"name": "Test Product", "price": 19.99});
        assert_eq!(extract_string(&hit, "name"), Some("Test Product".to_string()));
        assert_eq!(extract_string(&hit, "price"), Some("19.99".to_string()));
        assert_eq!(extract_string(&hit, "missing"), None);
    }

    #[test]
    fn test_extract_string_nested() {
        let hit = serde_json::json!({"hierarchy": {"lvl0": "Docs", "lvl1": "API"}});
        assert_eq!(extract_string(&hit, "hierarchy.lvl0"), Some("Docs".to_string()));
        assert_eq!(extract_string(&hit, "hierarchy.lvl1"), Some("API".to_string()));
        assert_eq!(extract_string(&hit, "hierarchy.lvl2"), None);
    }

    #[test]
    fn test_extract_string_array_first_string() {
        let hit = serde_json::json!({"tags": ["rust", "cli"]});
        assert_eq!(extract_string(&hit, "tags"), Some("rust".to_string()));
    }

    #[test]
    fn test_detect_docsearch() {
        let attrs = vec![
            "hierarchy.lvl0".to_string(),
            "hierarchy.lvl1".to_string(),
            "content".to_string(),
            "url".to_string(),
            "anchor".to_string(),
            "type".to_string(),
        ];
        let mapping = detect_mapping(&attrs);
        assert_eq!(mapping.profile, "docsearch");
        assert!(!mapping.hierarchy.is_empty());
        assert_eq!(mapping.description, Some("content".to_string()));
        assert_eq!(mapping.url, Some("url".to_string()));
    }

    #[test]
    fn test_detect_ecommerce() {
        let attrs = vec![
            "display_name".to_string(),
            "price".to_string(),
            "brand_name".to_string(),
            "link".to_string(),
            "short_description".to_string(),
            "image_url".to_string(),
        ];
        let mapping = detect_mapping(&attrs);
        assert_eq!(mapping.profile, "ecommerce");
        assert_eq!(mapping.title, Some("display_name".to_string()));
        assert_eq!(mapping.price, Some("price".to_string()));
        assert_eq!(mapping.brand, Some("brand_name".to_string()));
        assert_eq!(mapping.url, Some("link".to_string()));
        assert_eq!(mapping.description, Some("short_description".to_string()));
        assert_eq!(mapping.image, Some("image_url".to_string()));
    }

    #[test]
    fn test_detect_generic() {
        let attrs = vec![
            "title".to_string(),
            "body".to_string(),
            "slug".to_string(),
        ];
        let mapping = detect_mapping(&attrs);
        assert_eq!(mapping.profile, "generic");
        assert_eq!(mapping.title, Some("title".to_string()));
        assert_eq!(mapping.description, Some("body".to_string()));
        assert_eq!(mapping.url, Some("slug".to_string()));
    }

    #[test]
    fn test_collect_attribute_paths() {
        let value = serde_json::json!({
            "name": "test",
            "nested": {"a": 1, "b": {"c": 2}},
            "flat": 42
        });
        let mut paths = Vec::new();
        collect_attribute_paths(&value, "", &mut paths);
        paths.sort();
        assert!(paths.contains(&"name".to_string()));
        assert!(paths.contains(&"nested".to_string()));
        assert!(paths.contains(&"nested.a".to_string()));
        assert!(paths.contains(&"nested.b".to_string()));
        assert!(paths.contains(&"nested.b.c".to_string()));
        assert!(paths.contains(&"flat".to_string()));
    }

    #[test]
    fn test_first_match_case_insensitive() {
        let attrs = vec!["Display_Name".to_string(), "Price".to_string()];
        assert_eq!(
            first_match(&attrs, &["display_name"]),
            Some("Display_Name".to_string())
        );
    }

    #[test]
    fn test_default_docsearch_mapping() {
        let m = default_docsearch_mapping();
        assert_eq!(m.profile, "docsearch");
        assert_eq!(m.hierarchy.len(), 6);
        assert_eq!(m.url, Some("url".to_string()));
        assert_eq!(m.url_anchor, Some("anchor".to_string()));
        assert_eq!(m.description, Some("content".to_string()));
    }
}
