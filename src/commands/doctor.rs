use crate::context::AppContext;
use crate::discovery::llm_fallback;
use crate::error::AppResult;
use crate::registry::Registry;
use colored::Colorize;

pub async fn run(ctx: &AppContext) -> AppResult<()> {
    if ctx.presenter.is_json() {
        return run_json(ctx).await;
    }

    println!("{}", "algosearch doctor".bold());
    println!();

    // Check config directory
    let config_dir = ctx.registry_path.parent().unwrap();
    if config_dir.exists() {
        println!("  {} config dir: {}", "OK".green(), config_dir.display());
    } else {
        println!(
            "  {} config dir: {} (will be created on first use)",
            "--".yellow(),
            config_dir.display()
        );
    }

    // Check registry file
    if ctx.registry_path.exists() {
        match Registry::load(&ctx.registry_path) {
            Ok(reg) => {
                println!(
                    "  {} registry: {} site(s)",
                    "OK".green(),
                    reg.sites.len()
                );

                // Validate each site
                for (name, site) in &reg.sites {
                    if let Some(index) = site.indices.first() {
                        let url = format!(
                            "https://{}-dsn.algolia.net/1/indexes/{}/query",
                            index.app_id, index.index_name
                        );
                        let body = serde_json::json!({"query": "", "hitsPerPage": 0});
                        let result = ctx
                            .http_client
                            .post(&url)
                            .header("x-algolia-application-id", &index.app_id)
                            .header("x-algolia-api-key", &index.api_key)
                            .json(&body)
                            .send()
                            .await;

                        match result {
                            Ok(resp) if resp.status().is_success() => {
                                println!("  {} {}: valid", "OK".green(), name);
                            }
                            Ok(resp) if resp.status() == 403 => {
                                println!(
                                    "  {} {}: expired (403)",
                                    "!!".red(),
                                    name
                                );
                            }
                            Ok(resp) => {
                                println!(
                                    "  {} {}: error ({})",
                                    "!!".red(),
                                    name,
                                    resp.status()
                                );
                            }
                            Err(e) => {
                                println!(
                                    "  {} {}: unreachable ({})",
                                    "!!".red(),
                                    name,
                                    e
                                );
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("  {} registry: corrupt ({})", "!!".red(), e);
            }
        }
    } else {
        println!(
            "  {} registry: not created yet",
            "--".yellow()
        );
    }

    // Agent detection status
    println!();
    if ctx.agent_info.is_agent {
        println!(
            "  {} agent mode: active ({})",
            ">>".cyan(),
            ctx.agent_info
                .detected_by
                .as_deref()
                .unwrap_or("unknown")
        );
    } else {
        println!("  {} agent mode: inactive", "--".dimmed());
    }

    Ok(())
}

async fn run_json(ctx: &AppContext) -> AppResult<()> {
    let reg = Registry::load(&ctx.registry_path).unwrap_or_default();

    let mut sites_status = Vec::new();
    for (name, site) in &reg.sites {
        let status = if let Some(index) = site.indices.first() {
            let url = format!(
                "https://{}-dsn.algolia.net/1/indexes/{}/query",
                index.app_id, index.index_name
            );
            let body = serde_json::json!({"query": "", "hitsPerPage": 0});
            match ctx
                .http_client
                .post(&url)
                .header("x-algolia-application-id", &index.app_id)
                .header("x-algolia-api-key", &index.api_key)
                .json(&body)
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => "valid",
                Ok(resp) if resp.status() == 403 => "expired",
                _ => "unreachable",
            }
        } else {
            "no_indices"
        };

        sites_status.push(serde_json::json!({
            "name": name,
            "url": site.url,
            "status": status,
        }));
    }

    // Generate a dummy URL for fallback instructions
    let dummy_url = url::Url::parse("https://example.com").unwrap();

    let output = serde_json::json!({
        "config_dir": ctx.registry_path.parent().map(|p| p.display().to_string()),
        "registry_exists": ctx.registry_path.exists(),
        "sites": sites_status,
        "agent_mode": {
            "active": ctx.agent_info.is_agent,
            "detected_by": ctx.agent_info.detected_by,
        },
        "fallback_instructions_template": llm_fallback::generate_instructions(&dummy_url),
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    Ok(())
}
