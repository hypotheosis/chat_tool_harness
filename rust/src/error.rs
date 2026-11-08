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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoreError {
    InvalidInput,
    Configuration,
    Authentication,
    CredentialMissing,
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

impl CoreError {
    /// Every variant; `all_lists_every_variant` keeps it complete.
    pub const ALL: [Self; 14] = [
        Self::InvalidInput,
        Self::Configuration,
        Self::Authentication,
        Self::CredentialMissing,
        Self::RateLimited,
        Self::ToolUnsupported,
        Self::DiscoveryUnavailable,
        Self::ContextLength,
        Self::UnavailableTransport,
        Self::Protocol,
        Self::Limit,
        Self::Storage,
        Self::Cancelled,
        Self::Conflict,
    ];
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidInput => "invalid core input",
            Self::Configuration => "invalid core configuration",
            Self::Authentication => "core authentication failure",
            Self::CredentialMissing => crate::failure::CREDENTIAL_MISSING_REASON,
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

#[cfg(test)]
mod tests {
    use super::CoreError;

    #[test]
    fn all_lists_every_variant() {
        // Adding a variant breaks this match; add it to `ALL` too.
        let index = |e: &CoreError| match e {
            CoreError::InvalidInput => 0,
            CoreError::Configuration => 1,
            CoreError::Authentication => 2,
            CoreError::CredentialMissing => 3,
            CoreError::RateLimited => 4,
            CoreError::ToolUnsupported => 5,
            CoreError::DiscoveryUnavailable => 6,
            CoreError::ContextLength => 7,
            CoreError::UnavailableTransport => 8,
            CoreError::Protocol => 9,
            CoreError::Limit => 10,
            CoreError::Storage => 11,
            CoreError::Cancelled => 12,
            CoreError::Conflict => 13,
        };
        for (i, e) in CoreError::ALL.iter().enumerate() {
            assert_eq!(index(e), i);
        }
    }
}
