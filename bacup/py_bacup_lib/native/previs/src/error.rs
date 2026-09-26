use thiserror::Error;

#[derive(Debug, Error)]
pub enum PrevisError {
    /// Input is outside the CK-verified subset; the generator refuses rather
    /// than emitting plausible but unproven visibility data.
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

impl PrevisError {
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported(message.into())
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    /// Prefixes the message with the asset or record it came from.
    pub fn with_context(self, context: &str) -> Self {
        match self {
            Self::Unsupported(m) => Self::Unsupported(format!("{context}: {m}")),
            Self::Invalid(m) => Self::Invalid(format!("{context}: {m}")),
            other => other,
        }
    }

    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported(_))
    }
}

pub type Result<T> = std::result::Result<T, PrevisError>;
