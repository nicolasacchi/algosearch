mod agent;
mod cli;
mod commands;
mod context;
mod discovery;
mod display;
mod error;
mod http;
mod output;
mod registry;
pub mod schema;
mod search;

use clap::{CommandFactory, Parser};
use cli::{Cli, Commands};
use context::AppContext;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Handle completions and manpage before building AppContext (no HTTP client needed)
    match &cli.command {
        Commands::Completions(args) => {
            generate_completions(args);
            return;
        }
        Commands::Manpage => {
            generate_manpage();
            return;
        }
        _ => {}
    }

    let ctx = AppContext::new(&cli);

    let result = match &cli.command {
        Commands::Add(args) => commands::add::run(&ctx, args).await,
        Commands::Search(args) => commands::search::run(&ctx, args).await,
        Commands::Query(args) => commands::query::run(&ctx, args).await,
        Commands::Ls(args) => commands::list::run(&ctx, args),
        Commands::Refresh(args) => commands::refresh::run(&ctx, args).await,
        Commands::Remove(args) => commands::remove::run(&ctx, args),
        Commands::Discover(args) => commands::discover::run(&ctx, args).await,
        Commands::Facets(args) => commands::facets::run(&ctx, args).await,
        Commands::Export(args) => commands::export::run(&ctx, args).await,
        Commands::Doctor => commands::doctor::run(&ctx).await,
        Commands::Completions(_) | Commands::Manpage => unreachable!(),
    };

    if let Err(e) = result {
        ctx.presenter.error(&e);
        std::process::exit(1);
    }
}

fn generate_completions(args: &cli::CompletionsArgs) {
    use clap_complete::{generate, Shell};

    let shell = match args.shell {
        cli::ShellType::Bash => Shell::Bash,
        cli::ShellType::Zsh => Shell::Zsh,
        cli::ShellType::Fish => Shell::Fish,
        cli::ShellType::PowerShell => Shell::PowerShell,
    };

    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "algosearch", &mut std::io::stdout());
}

fn generate_manpage() {
    let cmd = Cli::command();
    let man = clap_mangen::Man::new(cmd);
    man.render(&mut std::io::stdout())
        .expect("failed to generate man page");
}
