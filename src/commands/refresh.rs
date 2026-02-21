use crate::cli::RefreshArgs;
use crate::context::AppContext;
use crate::discovery::{self, DiscoveryResult};
use crate::error::{AppError, AppResult};
use crate::registry::Registry;
use chrono::Utc;

pub async fn run(ctx: &AppContext, args: &RefreshArgs) -> AppResult<()> {
    let mut reg = Registry::load(&ctx.registry_path)?;

    let sites_to_refresh: Vec<String> = if args.all {
        reg.sites.keys().cloned().collect()
    } else {
        let name = args.site.as_ref().unwrap();
        if reg.get_site(name).is_none() {
            return Err(AppError::SiteNotFound {
                name: name.clone(),
                suggestion: Some("run 'algosearch ls' to see registered sites".to_string()),
            });
        }
        vec![name.clone()]
    };

    let mut refreshed = Vec::new();
    let mut failed = Vec::new();

    for name in &sites_to_refresh {
        let site = reg.get_site(name).unwrap().clone();
        let url = match url::Url::parse(&site.url) {
            Ok(u) => u,
            Err(_) => {
                failed.push(name.clone());
                continue;
            }
        };

        ctx.presenter
            .progress(&format!("refreshing {}...", name));

        match discovery::discover(&url, &ctx.http_client, ctx.verbose).await {
            Ok(DiscoveryResult::Found(discovered)) => {
                let mut updated = site.clone();
                updated.indices = discovered.indices;
                updated.last_verified = Utc::now();
                updated.discovery_method = discovered.discovery_method;
                reg.add_site(name.clone(), updated);
                refreshed.push(name.clone());
            }
            _ => {
                failed.push(name.clone());
            }
        }
    }

    reg.save(&ctx.registry_path)?;

    #[derive(serde::Serialize)]
    struct RefreshResult {
        refreshed: Vec<String>,
        failed: Vec<String>,
    }

    let result = RefreshResult { refreshed, failed };

    ctx.presenter.success(&result, |r| {
        let mut out = String::new();
        if !r.refreshed.is_empty() {
            out.push_str(&format!("Refreshed: {}\n", r.refreshed.join(", ")));
        }
        if !r.failed.is_empty() {
            out.push_str(&format!("Failed: {}\n", r.failed.join(", ")));
        }
        out
    });

    Ok(())
}
