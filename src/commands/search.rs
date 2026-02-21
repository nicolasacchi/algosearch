use crate::cli::SearchArgs;
use crate::context::AppContext;
use crate::display;
use crate::error::{AppError, AppResult};
use crate::registry::Registry;
use crate::search::{self, SearchHit, SearchResponse};

pub async fn run(ctx: &AppContext, args: &SearchArgs) -> AppResult<()> {
    let (site_opt, query) = args
        .resolve()
        .map_err(|e| AppError::Other(e))?;

    let reg = Registry::load(&ctx.registry_path)?;

    if args.all {
        return search_all(ctx, &reg, query, args).await;
    }

    let site_name = site_opt.unwrap();
    let site = reg.get_site(site_name).ok_or_else(|| AppError::SiteNotFound {
        name: site_name.to_string(),
        suggestion: Some("run 'algosearch ls' to see registered sites".to_string()),
    })?;

    let index = if let Some(idx_name) = &args.index {
        site.indices
            .iter()
            .find(|i| i.index_name == *idx_name)
            .ok_or_else(|| AppError::SearchFailed {
                message: format!("index '{}' not found for site '{}'", idx_name, site_name),
                suggestion: Some(format!(
                    "available indices: {}",
                    site.indices
                        .iter()
                        .map(|i| i.index_name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            })?
    } else {
        site.indices
            .iter()
            .find(|i| i.is_default)
            .or(site.indices.first())
            .ok_or_else(|| AppError::SearchFailed {
                message: format!("no indices found for site '{}'", site_name),
                suggestion: Some(format!("try: algosearch refresh {}", site_name)),
            })?
    };

    let filters: Vec<(String, String)> = args
        .filters
        .iter()
        .map(|f| (f.key.clone(), f.value.clone()))
        .collect();

    let raw = search::search_algolia(
        &ctx.http_client,
        &index.app_id,
        &index.api_key,
        &index.index_name,
        query,
        &filters,
        ctx.max_results,
    )
    .await;

    match raw {
        Ok(raw) => {
            let results: Vec<SearchHit> = raw.hits.iter().map(|h| h.to_search_hit()).collect();
            let response = SearchResponse {
                site: site_name.to_string(),
                query: query.to_string(),
                total_hits: raw.nb_hits,
                results,
            };
            ctx.presenter
                .success(&response, |r| display::format_search_results(r));
        }
        Err(AppError::AlgoliaApi { status: 403, .. }) => {
            if let Some(answer) = ctx
                .presenter
                .prompt("Credentials may have expired. Re-discover now? [y/N] ")
            {
                if answer.to_lowercase() == "y" {
                    eprintln!("Run: algosearch refresh {}", site_name);
                }
            }
            return Err(AppError::AlgoliaApi {
                status: 403,
                message: "forbidden — API key may have expired".to_string(),
                suggestion: Some(format!("try: algosearch refresh {}", site_name)),
            });
        }
        Err(e) => return Err(e),
    }

    Ok(())
}

async fn search_all(
    ctx: &AppContext,
    reg: &Registry,
    query: &str,
    args: &SearchArgs,
) -> AppResult<()> {
    if reg.sites.is_empty() {
        return Err(AppError::SearchFailed {
            message: "no sites registered".to_string(),
            suggestion: Some("run 'algosearch add <URL>' to register a site".to_string()),
        });
    }

    let filters: Vec<(String, String)> = args
        .filters
        .iter()
        .map(|f| (f.key.clone(), f.value.clone()))
        .collect();

    let mut handles = Vec::new();

    for (name, site) in &reg.sites {
        if let Some(index) = site.indices.iter().find(|i| i.is_default).or(site.indices.first()) {
            let client = ctx.http_client.clone();
            let app_id = index.app_id.clone();
            let api_key = index.api_key.clone();
            let index_name = index.index_name.clone();
            let query = query.to_string();
            let filters = filters.clone();
            let max_results = ctx.max_results;
            let site_name = name.clone();

            let handle = tokio::spawn(async move {
                let result = search::search_algolia(
                    &client,
                    &app_id,
                    &api_key,
                    &index_name,
                    &query,
                    &filters,
                    max_results,
                )
                .await;
                (site_name, result)
            });
            handles.push(handle);
        }
    }

    let mut all_responses: Vec<SearchResponse> = Vec::new();
    for handle in handles {
        if let Ok((site_name, Ok(raw))) = handle.await {
            let results: Vec<SearchHit> = raw.hits.iter().map(|h| h.to_search_hit()).collect();
            all_responses.push(SearchResponse {
                site: site_name,
                query: query.to_string(),
                total_hits: raw.nb_hits,
                results,
            });
        }
    }

    ctx.presenter.success(&all_responses, |responses| {
        responses
            .iter()
            .map(|r| display::format_search_results(r))
            .collect::<Vec<_>>()
            .join("\n")
    });

    Ok(())
}
