use crate::cli::RemoveArgs;
use crate::context::AppContext;
use crate::error::{AppError, AppResult};
use crate::registry::Registry;

pub fn run(ctx: &AppContext, args: &RemoveArgs) -> AppResult<()> {
    let mut reg = Registry::load(&ctx.registry_path)?;

    if reg.get_site(&args.site).is_none() {
        return Err(AppError::SiteNotFound {
            name: args.site.clone(),
            suggestion: Some("run 'algosearch ls' to see registered sites".to_string()),
        });
    }

    reg.remove_site(&args.site);
    reg.save(&ctx.registry_path)?;

    #[derive(serde::Serialize)]
    struct RemoveResult {
        removed: String,
    }

    let result = RemoveResult {
        removed: args.site.clone(),
    };

    ctx.presenter.success(&result, |r| {
        format!("Removed '{}'", r.removed)
    });

    Ok(())
}
