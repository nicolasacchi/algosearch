use crate::cli::{ExportArgs, OutputFormat};
use crate::context::AppContext;
use crate::error::{AppError, AppResult};
use crate::registry::Registry;
use crate::search;
use std::collections::HashSet;
use std::io::Write;

pub async fn run(ctx: &AppContext, args: &ExportArgs) -> AppResult<()> {
    let reg = Registry::load(&ctx.registry_path)?;

    let site = reg.get_site(&args.site).ok_or_else(|| AppError::SiteNotFound {
        name: args.site.clone(),
        suggestion: Some("run 'algosearch ls' to see registered sites".to_string()),
    })?;

    let index = if let Some(idx_name) = &args.index {
        site.indices
            .iter()
            .find(|i| i.index_name == *idx_name)
            .ok_or_else(|| AppError::SearchFailed {
                message: format!("index '{}' not found for site '{}'", idx_name, args.site),
                suggestion: None,
            })?
    } else {
        site.indices
            .iter()
            .find(|i| i.is_default)
            .or(site.indices.first())
            .ok_or_else(|| AppError::SearchFailed {
                message: format!("no indices found for site '{}'", args.site),
                suggestion: Some(format!("try: algosearch refresh {}", args.site)),
            })?
    };

    // Parse requested fields
    let fields: Option<Vec<&str>> = args
        .fields
        .as_ref()
        .map(|f| f.split(',').map(|s| s.trim()).collect());

    // Base filters from CLI
    let base_filters: Vec<String> = args
        .filters
        .iter()
        .map(|f| format!("{}:{}", f.key, f.value))
        .collect();

    // Open output
    let mut writer: Box<dyn Write> = if let Some(path) = &args.output {
        Box::new(
            std::fs::File::create(path)
                .map_err(|e| AppError::Other(format!("cannot create output file '{}': {}", path, e)))?,
        )
    } else {
        Box::new(std::io::stdout().lock())
    };

    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut total_exported: usize = 0;
    let mut total_api_calls: usize = 0;
    let mut csv_header_written = false;

    if let Some(partition_attr) = &args.partition_by {
        // Partitioned export: fetch facet values, then query each partition
        eprintln!("Discovering partitions via facet '{}'...", partition_attr);

        let facet_values = search::fetch_facets(
            &ctx.http_client,
            &index.app_id,
            &index.api_key,
            &index.index_name,
            partition_attr,
            10000, // Get all partition values
        )
        .await?;
        total_api_calls += 1;

        eprintln!(
            "Found {} partitions ({} total records)",
            facet_values.len(),
            facet_values.iter().map(|(_, c)| c).sum::<u64>()
        );

        // Build multi-query batches from partition values
        let batch_size = args.batch_size.max(1).min(20); // Clamp to 1..20
        let partitions: Vec<&(String, u64)> = facet_values.iter().collect();

        for chunk in partitions.chunks(batch_size) {
            let mut queries: Vec<search::MultiQueryRequest> = Vec::new();

            for (partition_value, _count) in chunk.iter() {
                let mut filters = base_filters.clone();
                filters.push(format!("{}:{}", partition_attr, partition_value));

                let params = search::build_query_params(
                    "",
                    &filters,
                    1000, // Max per partition
                    0,
                    fields.as_ref().map(|f| f.as_slice()),
                );

                queries.push(search::MultiQueryRequest {
                    index_name: index.index_name.clone(),
                    params,
                });
            }

            let multi_resp = search::multi_query(
                &ctx.http_client,
                &index.app_id,
                &index.api_key,
                &queries,
            )
            .await?;
            total_api_calls += 1;

            for result in &multi_resp.results {
                for hit in &result.hits {
                    let object_id = hit
                        .get("objectID")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    if object_id.is_empty() || seen_ids.contains(&object_id) {
                        continue;
                    }
                    seen_ids.insert(object_id);

                    write_record(
                        &mut writer,
                        hit,
                        &args.format,
                        fields.as_ref(),
                        &mut csv_header_written,
                    )?;
                    total_exported += 1;
                }
            }

            if total_api_calls % 10 == 0 || total_api_calls <= 2 {
                eprintln!(
                    "  {} API calls, {} records exported...",
                    total_api_calls, total_exported
                );
            }
        }
    } else {
        // Non-partitioned export: simple pagination (limited to 1000 by Algolia)
        eprintln!(
            "Exporting without partitioning (max 1000 records)."
        );
        eprintln!(
            "Hint: use --partition-by <attribute> for indexes with >1000 records"
        );

        let mut page: u32 = 0;
        loop {
            let raw = search::search_algolia_paged(
                &ctx.http_client,
                &index.app_id,
                &index.api_key,
                &index.index_name,
                "",
                &base_filters
                    .iter()
                    .map(|f| {
                        let parts: Vec<&str> = f.splitn(2, ':').collect();
                        (parts[0].to_string(), parts.get(1).unwrap_or(&"").to_string())
                    })
                    .collect::<Vec<_>>(),
                ctx.max_results.max(100), // Use at least 100 per page for export
                page,
            )
            .await?;
            total_api_calls += 1;

            if raw.hits.is_empty() {
                break;
            }

            for hit in &raw.hits {
                let object_id = hit
                    .get("objectID")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                if !object_id.is_empty() && seen_ids.contains(&object_id) {
                    continue;
                }
                if !object_id.is_empty() {
                    seen_ids.insert(object_id);
                }

                write_record(
                    &mut writer,
                    hit,
                    &args.format,
                    fields.as_ref(),
                    &mut csv_header_written,
                )?;
                total_exported += 1;
            }

            let nb_pages = raw.nb_pages.unwrap_or(1);
            page += 1;
            if page as u64 >= nb_pages {
                break;
            }
        }
    }

    // Flush and close
    writer.flush()?;

    eprintln!(
        "Done: {} records exported in {} API calls",
        total_exported, total_api_calls
    );

    if let Some(path) = &args.output {
        eprintln!("Output: {}", path);
    }

    Ok(())
}

/// Write a single Algolia hit record in the chosen format.
fn write_record(
    writer: &mut dyn Write,
    hit: &serde_json::Value,
    format: &OutputFormat,
    fields: Option<&Vec<&str>>,
    csv_header_written: &mut bool,
) -> AppResult<()> {
    match format {
        OutputFormat::Jsonl => {
            let output = if let Some(fields) = fields {
                filter_fields(hit, fields)
            } else {
                strip_internal(hit)
            };
            writeln!(writer, "{}", serde_json::to_string(&output).unwrap_or_default())?;
        }
        OutputFormat::Json => {
            // JSON format writes one object per line (same as JSONL for streaming)
            // Full JSON array would require buffering all records in memory
            let output = if let Some(fields) = fields {
                filter_fields(hit, fields)
            } else {
                strip_internal(hit)
            };
            writeln!(writer, "{}", serde_json::to_string_pretty(&output).unwrap_or_default())?;
        }
        OutputFormat::Csv => {
            if let Some(obj) = hit.as_object() {
                let field_list: Vec<&str> = if let Some(fields) = fields {
                    fields.clone()
                } else {
                    // Use all non-internal fields from this record
                    obj.keys()
                        .filter(|k| !k.starts_with('_') && *k != "objectID")
                        .map(|k| k.as_str())
                        .collect()
                };

                // Write CSV header on first record
                if !*csv_header_written {
                    writeln!(writer, "{}", field_list.join(","))?;
                    *csv_header_written = true;
                }

                let values: Vec<String> = field_list
                    .iter()
                    .map(|field| {
                        let val = extract_flat_value(hit, field);
                        csv_escape(&val)
                    })
                    .collect();

                writeln!(writer, "{}", values.join(","))?;
            }
        }
    }
    Ok(())
}

/// Extract a value from a JSON hit, flattening to a string for CSV.
fn extract_flat_value(hit: &serde_json::Value, path: &str) -> String {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = hit;

    for part in &parts {
        match current {
            serde_json::Value::Object(map) => {
                if let Some(next) = map.get(*part) {
                    current = next;
                } else {
                    return String::new();
                }
            }
            _ => return String::new(),
        }
    }

    match current {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(), // Arrays/objects as JSON string
    }
}

/// Escape a value for CSV output.
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Filter a hit to only include specified fields.
fn filter_fields(hit: &serde_json::Value, fields: &[&str]) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(source) = hit.as_object() {
        for field in fields {
            // Support dotted paths by traversing
            let parts: Vec<&str> = field.split('.').collect();
            if parts.len() == 1 {
                if let Some(val) = source.get(*field) {
                    obj.insert(field.to_string(), val.clone());
                }
            } else {
                // For dotted paths, extract the value and flatten
                let mut current: &serde_json::Value = hit;
                let mut found = true;
                for part in &parts {
                    if let Some(next) = current.get(*part) {
                        current = next;
                    } else {
                        found = false;
                        break;
                    }
                }
                if found {
                    obj.insert(field.to_string(), current.clone());
                }
            }
        }
    }
    serde_json::Value::Object(obj)
}

/// Strip Algolia internal fields from a hit.
fn strip_internal(hit: &serde_json::Value) -> serde_json::Value {
    let mut cleaned = hit.clone();
    if let Some(obj) = cleaned.as_object_mut() {
        obj.retain(|k, _| !k.starts_with('_') && k != "objectID");
    }
    cleaned
}
