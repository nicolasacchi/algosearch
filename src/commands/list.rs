use crate::cli::LsArgs;
use crate::context::AppContext;
use crate::error::{AppError, AppResult};
use crate::registry::Registry;
use colored::Colorize;

pub fn run(ctx: &AppContext, args: &LsArgs) -> AppResult<()> {
    let reg = Registry::load(&ctx.registry_path)?;

    if let Some(site_name) = &args.site {
        // Show details of a specific site
        let site = reg.get_site(site_name).ok_or_else(|| AppError::SiteNotFound {
            name: site_name.clone(),
            suggestion: Some("run 'algosearch ls' to see registered sites".to_string()),
        })?;

        ctx.presenter.success(site, |s| {
            let mut out = String::new();
            out.push_str(&format!("{}\n", s.name.bold()));
            out.push_str(&format!("  url:        {}\n", s.url));
            out.push_str(&format!("  discovered: {}\n", s.discovered_at.format("%Y-%m-%d %H:%M")));
            out.push_str(&format!("  verified:   {}\n", s.last_verified.format("%Y-%m-%d %H:%M")));
            out.push_str(&format!("  method:     {}\n", s.discovery_method));
            out.push_str(&format!("  indices:    {}\n", s.indices.len()));

            for (i, idx) in s.indices.iter().enumerate() {
                let default_marker = if idx.is_default { " (default)" } else { "" };
                out.push_str(&format!(
                    "\n  [{}]{}\n    app_id:     {}\n    index:      {}\n",
                    i, default_marker, idx.app_id, idx.index_name
                ));
                if let Some(count) = idx.record_count {
                    out.push_str(&format!("    records:    {}\n", count));
                }
                if let Some(facets) = &idx.facets {
                    for (k, v) in facets {
                        out.push_str(&format!("    facet {}: {}\n", k, v.join(", ")));
                    }
                }
            }
            out
        });
    } else {
        // List all sites
        if reg.sites.is_empty() {
            ctx.presenter.success(&reg.sites, |_| {
                "No sites registered. Run 'algosearch add <URL>' to get started.".to_string()
            });
            return Ok(());
        }

        let mut sites: Vec<_> = reg.sites.iter().collect();
        sites.sort_by_key(|(name, _)| (*name).clone());

        #[derive(serde::Serialize)]
        struct SiteList {
            sites: Vec<SiteSummary>,
        }
        #[derive(serde::Serialize)]
        struct SiteSummary {
            name: String,
            url: String,
            indices: usize,
        }

        let list = SiteList {
            sites: sites
                .iter()
                .map(|(name, site)| SiteSummary {
                    name: name.to_string(),
                    url: site.url.clone(),
                    indices: site.indices.len(),
                })
                .collect(),
        };

        ctx.presenter.success(&list, |l| {
            let mut out = String::new();
            for s in &l.sites {
                out.push_str(&format!(
                    "  {} {} ({} index{})\n",
                    s.name.bold(),
                    s.url.dimmed(),
                    s.indices,
                    if s.indices == 1 { "" } else { "es" }
                ));
            }
            out
        });
    }

    Ok(())
}
