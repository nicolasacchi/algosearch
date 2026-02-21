use crate::agent;
use crate::cli::Cli;
use crate::http;
use crate::output::Presenter;
use std::path::PathBuf;

pub struct AppContext {
    pub http_client: reqwest::Client,
    pub presenter: Presenter,
    pub registry_path: PathBuf,
    pub verbose: bool,
    pub max_results: u32,
    pub agent_info: agent::AgentInfo,
}

impl AppContext {
    pub fn new(cli: &Cli) -> Self {
        let agent_info = agent::detect(cli.agent, cli.json);
        let http_client = http::build_client(cli.timeout)
            .expect("failed to build HTTP client");
        let presenter = Presenter::new(cli.json, agent_info.is_agent, cli.verbose);
        let registry_path = default_registry_path();

        AppContext {
            http_client,
            presenter,
            registry_path,
            verbose: cli.verbose,
            max_results: cli.max_results,
            agent_info,
        }
    }
}

fn default_registry_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("algosearch")
        .join("sites.json")
}
