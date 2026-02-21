use clap::{Parser, Subcommand, Args, ValueEnum};
use std::fmt;
use std::str::FromStr;

#[derive(Parser)]
#[command(
    name = "algosearch",
    version,
    about = "Discover Algolia DocSearch credentials and search documentation sites",
    long_about = None,
    propagate_version = true,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output as JSON (auto-enabled when LLM agent detected)
    #[arg(long, global = true)]
    pub json: bool,

    /// Show discovery progress
    #[arg(long, short, global = true)]
    pub verbose: bool,

    /// Skip registry, always discover fresh
    #[arg(long, global = true)]
    pub no_cache: bool,

    /// HTTP timeout in seconds
    #[arg(long, global = true, default_value = "10")]
    pub timeout: u64,

    /// Search hits per page
    #[arg(long, global = true, default_value = "10")]
    pub max_results: u32,

    /// Force agent mode (machine-friendly output)
    #[arg(long, global = true)]
    pub agent: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Discover and register a documentation site
    Add(AddArgs),

    /// Search a registered site
    Search(SearchArgs),

    /// One-shot: discover + search without saving
    Query(QueryArgs),

    /// List registered sites
    Ls(LsArgs),

    /// Re-discover credentials for a site
    Refresh(RefreshArgs),

    /// Remove a site from the registry
    Remove(RemoveArgs),

    /// Discover credentials without saving
    Discover(DiscoverArgs),

    /// Run diagnostics
    Doctor,

    /// Generate shell completions
    Completions(CompletionsArgs),

    /// Generate man page
    #[command(hide = true)]
    Manpage,
}

#[derive(Args)]
pub struct CompletionsArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: ShellType,
}

#[derive(Clone, ValueEnum)]
pub enum ShellType {
    Bash,
    Zsh,
    Fish,
    #[value(name = "powershell")]
    PowerShell,
}

#[derive(Args)]
pub struct AddArgs {
    /// Documentation site URL
    pub url: String,

    /// Custom alias for the site
    #[arg(long)]
    pub name: Option<String>,

    /// Manual credentials (appId:apiKey:indexName)
    #[arg(long)]
    pub credentials: Option<Credentials>,
}

#[derive(Args)]
pub struct SearchArgs {
    /// Site to search, or query when --all is used
    pub first: String,

    /// Search query (when site is provided as first arg)
    pub second: Option<String>,

    /// Search all registered sites (first arg becomes the query)
    #[arg(long)]
    pub all: bool,

    /// Search a specific index (if site has multiple)
    #[arg(long)]
    pub index: Option<String>,

    /// Facet filter (key:value), can be repeated
    #[arg(long = "filter", value_name = "KEY:VALUE")]
    pub filters: Vec<Filter>,
}

impl SearchArgs {
    /// Resolve site and query from positional args.
    /// With --all: first=query, second=ignored
    /// Without --all: first=site, second=query (required)
    pub fn resolve(&self) -> Result<(Option<&str>, &str), String> {
        if self.all {
            Ok((None, &self.first))
        } else {
            match &self.second {
                Some(query) => Ok((Some(&self.first), query)),
                None => Err("missing <query> argument. Usage: algosearch search <site> <query>".to_string()),
            }
        }
    }
}

#[derive(Args)]
pub struct QueryArgs {
    /// Documentation site URL
    pub url: String,

    /// Search query
    pub query: String,

    /// Facet filter (key:value), can be repeated
    #[arg(long = "filter", value_name = "KEY:VALUE")]
    pub filters: Vec<Filter>,
}

#[derive(Args)]
pub struct LsArgs {
    /// Show details of a specific site
    pub site: Option<String>,
}

#[derive(Args)]
pub struct RefreshArgs {
    /// Site to refresh
    #[arg(required_unless_present = "all")]
    pub site: Option<String>,

    /// Refresh all registered sites
    #[arg(long)]
    pub all: bool,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// Site to remove
    pub site: String,
}

#[derive(Args)]
pub struct DiscoverArgs {
    /// Documentation site URL
    pub url: String,
}

/// Manual credentials in appId:apiKey:indexName format.
#[derive(Debug, Clone)]
pub struct Credentials {
    pub app_id: String,
    pub api_key: String,
    pub index_name: String,
}

impl FromStr for Credentials {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        if parts.len() != 3 {
            return Err("expected format: appId:apiKey:indexName".to_string());
        }
        Ok(Credentials {
            app_id: parts[0].to_string(),
            api_key: parts[1].to_string(),
            index_name: parts[2].to_string(),
        })
    }
}

impl fmt::Display for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.app_id, self.api_key, self.index_name)
    }
}

/// Facet filter in key:value format.
#[derive(Debug, Clone)]
pub struct Filter {
    pub key: String,
    pub value: String,
}

impl FromStr for Filter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (key, value) = s
            .split_once(':')
            .ok_or_else(|| "expected format: key:value".to_string())?;
        Ok(Filter {
            key: key.to_string(),
            value: value.to_string(),
        })
    }
}

impl fmt::Display for Filter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.key, self.value)
    }
}
