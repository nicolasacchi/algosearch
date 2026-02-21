use crate::cli::AddArgs;
use crate::context::AppContext;
use crate::discovery::{self, DiscoveryResult};
use crate::error::{AppError, AppResult};
use crate::registry::{self, RegisteredSite, Registry};
use chrono::Utc;

pub async fn run(ctx: &AppContext, args: &AddArgs) -> AppResult<()> {
    let url = url::Url::parse(&args.url)
        .map_err(|e| AppError::InvalidUrl(e.to_string()))?;

    let site_name = args
        .name
        .clone()
        .or_else(|| registry::derive_site_name(&args.url))
        .ok_or_else(|| AppError::InvalidUrl("cannot derive site name from URL".to_string()))?;

    let (indices, method) = if let Some(creds) = &args.credentials {
        // Manual credentials — still validate them
        ctx.presenter.progress("validating credentials...");
        let candidate = discovery::CandidateCredentials {
            app_id: creds.app_id.clone(),
            api_key: creds.api_key.clone(),
            index_name: creds.index_name.clone(),
        };
        let validated =
            discovery::validate::validate_candidates(&[candidate], &ctx.http_client).await?;
        if validated.is_empty() {
            return Err(AppError::ValidationFailed {
                app_id: creds.app_id.clone(),
                index_name: creds.index_name.clone(),
                reason: "credentials did not pass validation".to_string(),
                suggestion: Some("check that the appId, apiKey, and indexName are correct".to_string()),
            });
        }
        (validated, "manual".to_string())
    } else {
        // Auto-discover
        ctx.presenter.progress("discovering credentials...");
        match discovery::discover(&url, &ctx.http_client, ctx.verbose).await? {
            DiscoveryResult::Found(site) => (site.indices, site.discovery_method),
            DiscoveryResult::LlmFallback(instructions) => {
                if ctx.presenter.is_json() {
                    println!("{}", serde_json::to_string_pretty(&instructions).unwrap());
                } else {
                    eprintln!("Could not auto-discover credentials at {}", args.url);
                    eprintln!(
                        "Try: algosearch add {} --credentials <appId>:<apiKey>:<indexName>",
                        args.url
                    );
                }
                return Err(AppError::DiscoveryFailed {
                    url: args.url.clone(),
                    suggestion: Some(format!(
                        "try: algosearch add {} --credentials <appId>:<apiKey>:<indexName>",
                        args.url
                    )),
                });
            }
        }
    };

    let now = Utc::now();
    let site = RegisteredSite {
        url: url.to_string(),
        name: site_name.clone(),
        indices,
        discovered_at: now,
        last_verified: now,
        discovery_method: method,
    };

    let mut reg = Registry::load(&ctx.registry_path)?;
    reg.add_site(site_name.clone(), site.clone());
    reg.save(&ctx.registry_path)?;

    ctx.presenter.success(&site, |s| {
        format!(
            "Added '{}' ({}) with {} index(es)",
            site_name,
            s.url,
            s.indices.len()
        )
    });

    Ok(())
}
