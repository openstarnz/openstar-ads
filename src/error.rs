use ads_client as ads;
use tokio::sync::mpsc;

use crate::SymbolTypeTreeError;

#[derive(Debug, thiserror::Error)]
pub enum AdsError {
    #[error("ADS error {0}")]
    Ads(#[from] ads::AdsError),

    /// An IO error occurred.
    #[error("{0}: {1}")]
    Io(&'static str, std::io::Error),

    /// An unexpected or inconsistent reply was received.
    #[error("{0}: {1} ({2})")]
    Reply(&'static str, &'static str, u32),

    #[error("Symbol Type Tree error {0}")]
    SymbolTypeTree(#[from] SymbolTypeTreeError),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, AdsError>;

pub(crate) trait ErrContext {
    type Success;
    fn ctx(self, context: &'static str) -> Result<Self::Success>;
}

impl<T> ErrContext for std::result::Result<T, std::io::Error> {
    type Success = T;
    fn ctx(self, context: &'static str) -> Result<Self::Success> {
        self.map_err(|e| AdsError::Io(context, e))
    }
}
