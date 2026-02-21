use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("no algolia credentials found at {url}")]
    DiscoveryFailed {
        url: String,
        suggestion: Option<String>,
    },

    #[error("site '{name}' not found in registry")]
    SiteNotFound {
        name: String,
        suggestion: Option<String>,
    },

    #[error("validation failed for {app_id}/{index_name}: {reason}")]
    ValidationFailed {
        app_id: String,
        index_name: String,
        reason: String,
        suggestion: Option<String>,
    },

    #[error("search failed: {message}")]
    SearchFailed {
        message: String,
        suggestion: Option<String>,
    },

    #[error("algolia API error (HTTP {status}): {message}")]
    AlgoliaApi {
        status: u16,
        message: String,
        suggestion: Option<String>,
    },

    #[error("registry error: {0}")]
    Registry(String),

    #[error("file lock contention: {0}")]
    LockContention(String),

    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn suggestion(&self) -> Option<&str> {
        match self {
            AppError::DiscoveryFailed { suggestion, .. } => suggestion.as_deref(),
            AppError::SiteNotFound { suggestion, .. } => suggestion.as_deref(),
            AppError::ValidationFailed { suggestion, .. } => suggestion.as_deref(),
            AppError::SearchFailed { suggestion, .. } => suggestion.as_deref(),
            AppError::AlgoliaApi { suggestion, .. } => suggestion.as_deref(),
            _ => None,
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
