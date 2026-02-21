use crate::cli::DiscoverArgs;
use crate::context::AppContext;
use crate::discovery::{self, DiscoveryResult};
use crate::error::{AppError, AppResult};

pub async fn run(ctx: &AppContext, args: &DiscoverArgs) -> AppResult<()> {
    let url = url::Url::parse(&args.url)
        .map_err(|e| AppError::InvalidUrl(e.to_string()))?;

    ctx.presenter.progress("discovering credentials...");

    let result = discovery::discover(&url, &ctx.http_client, ctx.verbose).await?;

    match result {
        DiscoveryResult::Found(site) => {
            ctx.presenter.success(&site, |s| {
                let mut out = format!("Found {} index(es) at {}\n", s.indices.len(), s.url);
                for idx in &s.indices {
                    out.push_str(&format!(
                        "\n  app_id:     {}\n  api_key:    {}\n  index_name: {}\n",
                        idx.app_id, idx.api_key, idx.index_name
                    ));
                    if let Some(count) = idx.record_count {
                        out.push_str(&format!("  records:    {}\n", count));
                    }
                    if let Some(facets) = &idx.facets {
                        for (k, v) in facets {
                            out.push_str(&format!("  facet {}: {}\n", k, v.join(", ")));
                        }
                    }
                }
                out
            });
        }
        DiscoveryResult::LlmFallback(instructions) => {
            if ctx.presenter.is_json() {
                println!("{}", serde_json::to_string_pretty(&instructions).unwrap());
            } else {
                eprintln!("Could not auto-discover credentials at {}", args.url);
                eprintln!("Try using browser devtools to manually extract the credentials,");
                eprintln!("then run: algosearch add {} --credentials <appId>:<apiKey>:<indexName>", args.url);
            }
        }
    }

    Ok(())
}
