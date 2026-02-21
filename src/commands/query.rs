use crate::cli::QueryArgs;
use crate::context::AppContext;
use crate::discovery::{self, DiscoveryResult};
use crate::display;
use crate::error::{AppError, AppResult};
use crate::search::{self, SearchHit, SearchResponse};

pub async fn run(ctx: &AppContext, args: &QueryArgs) -> AppResult<()> {
    let url =
        url::Url::parse(&args.url).map_err(|e| AppError::InvalidUrl(e.to_string()))?;

    ctx.presenter.progress("discovering credentials...");

    let discovered = match discovery::discover(&url, &ctx.http_client, ctx.verbose).await? {
        DiscoveryResult::Found(site) => site,
        DiscoveryResult::LlmFallback(instructions) => {
            if ctx.presenter.is_json() {
                println!("{}", serde_json::to_string_pretty(&instructions).unwrap());
            }
            return Err(AppError::DiscoveryFailed {
                url: args.url.clone(),
                suggestion: Some("auto-discovery failed — see fallback instructions".to_string()),
            });
        }
    };

    let index = discovered.indices.first().ok_or_else(|| AppError::DiscoveryFailed {
        url: args.url.clone(),
        suggestion: Some("no valid indices found".to_string()),
    })?;

    let filters: Vec<(String, String)> = args
        .filters
        .iter()
        .map(|f| (f.key.clone(), f.value.clone()))
        .collect();

    ctx.presenter.progress("searching...");

    let raw = search::search_algolia(
        &ctx.http_client,
        &index.app_id,
        &index.api_key,
        &index.index_name,
        &args.query,
        &filters,
        ctx.max_results,
    )
    .await?;

    let site_name = crate::registry::derive_site_name(&args.url)
        .unwrap_or_else(|| "unknown".to_string());

    let results: Vec<SearchHit> = raw.hits.iter().map(|h| h.to_search_hit()).collect();
    let response = SearchResponse {
        site: site_name,
        query: args.query.clone(),
        total_hits: raw.nb_hits,
        results,
    };

    ctx.presenter
        .success(&response, |r| display::format_search_results(r));

    Ok(())
}
