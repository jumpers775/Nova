use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum ServiceError {
    ProviderError(String),
    NotFound(String),
    AuthenticationError(String),
    NetworkError(String),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceError::ProviderError(msg) => write!(f, "Provider error: {}", msg),
            ServiceError::NotFound(msg) => write!(f, "Not found: {}", msg),
            ServiceError::AuthenticationError(msg) => write!(f, "Authentication error: {}", msg),
            ServiceError::NetworkError(msg) => write!(f, "Network error: {}", msg),
        }
    }
}

impl Error for ServiceError {}
