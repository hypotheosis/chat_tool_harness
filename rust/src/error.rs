use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PdfFailure {
    Busy,
    Limit,
    InvalidInput,
    UnsupportedEncrypted,
    Timeout,
    WorkerDied,
    Protocol,
    Unavailable,
    Cancelled,
}

#[derive(Debug)]
pub struct AppError(pub PdfFailure);

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PDF extraction failed: {:?}", self.0)
    }
}

impl std::error::Error for AppError {}

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    InvalidInput,
    Configuration,
    Authentication,
    RateLimited,
    ToolUnsupported,
    DiscoveryUnavailable,
    ContextLength,
    UnavailableTransport,
    Protocol,
    Limit,
    Storage,
    Cancelled,
    Conflict,
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidInput => "invalid core input",
            Self::Configuration => "invalid core configuration",
            Self::Authentication => "core authentication failure",
            Self::RateLimited => "provider rate limited",
            Self::ToolUnsupported => "provider structured tools unsupported",
            Self::DiscoveryUnavailable => "model discovery unavailable",
            Self::ContextLength => {
                "provider context window exceeded; shorten or summarize explicitly"
            }
            Self::UnavailableTransport => "core transport unavailable",
            Self::Protocol => "core protocol failure",
            Self::Limit => "core limit exceeded",
            Self::Storage => "core storage failure",
            Self::Cancelled => "core operation cancelled",
            Self::Conflict => "core conflict",
        };
        f.write_str(message)
    }
}
impl std::error::Error for CoreError {}
pub type CoreResult<T> = Result<T, CoreError>;
