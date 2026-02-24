use crate::cli::FacetsArgs;
use crate::context::AppContext;
use crate::error::{AppError, AppResult};
use crate::registry::Registry;
use crate::search;
use colored::Colorize;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct FacetsResponse {
    site: String,
    attribute: String,
    total_values: usize,
    values: Vec<FacetValue>,
}

#[derive(Debug, Serialize)]
struct FacetValue {
    value: String,
    count: u64,
}

pub async fn run(ctx: &AppContext, args: &FacetsArgs) -> AppResult<()> {
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

    ctx.presenter.progress(&format!(
        "fetching facet values for '{}' on {}/{}...",
        args.attribute, args.site, index.index_name
    ));

    let facet_values = search::fetch_facets(
        &ctx.http_client,
        &index.app_id,
        &index.api_key,
        &index.index_name,
        &args.attribute,
        args.max_values,
    )
    .await?;

    let response = FacetsResponse {
        site: args.site.clone(),
        attribute: args.attribute.clone(),
        total_values: facet_values.len(),
        values: facet_values
            .into_iter()
            .map(|(value, count)| FacetValue { value, count })
            .collect(),
    };

    ctx.presenter.success(&response, |r| format_facets(r));

    Ok(())
}

fn format_facets(resp: &FacetsResponse) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "{} facet '{}' — {} values\n\n",
        format!("[{}]", resp.site).cyan().bold(),
        resp.attribute.bold(),
        resp.total_values
    ));

    // Find max count for alignment
    let max_count_width = resp
        .values
        .iter()
        .map(|v| v.count.to_string().len())
        .max()
        .unwrap_or(1);

    for fv in &resp.values {
        out.push_str(&format!(
            "  {:>width$}  {}\n",
            fv.count.to_string().dimmed(),
            fv.value,
            width = max_count_width
        ));
    }

    out
}
