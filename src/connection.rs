use std::sync::Arc;

use tokio::net::ToSocketAddrs;
use tracing::{info, warn};

use crate::{core, AmsAddr, Error, Result, Timeouts};

#[derive(Debug, Default)]
pub enum AdsConnection {
    Connected(Arc<core::Client>),
    #[default]
    Disconnected,
}

impl AdsConnection {
    pub async fn connect<RouterAddr: ToSocketAddrs>(
        &mut self,
        router: RouterAddr,
        target: AmsAddr,
        source: Option<AmsAddr>,
        timeouts: Timeouts,
    ) -> Result<()> {
        match self {
            AdsConnection::Connected(_) => {
                info!("Attempted to connect to PLC but it is already connected!")
            }
            AdsConnection::Disconnected => {
                let client = core::Client::new(router, target, source, timeouts).await?;

                *self = AdsConnection::Connected(client);
            }
        }

        Ok(())
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        match self {
            AdsConnection::Connected(client) => {
                client.close().await;

                *self = AdsConnection::Disconnected;

                info!("PLC connection was dropped.");
            }
            AdsConnection::Disconnected => {
                // Already disconnected...
            }
        }

        Ok(())
    }

    pub fn client(&self) -> Option<&core::Client> {
        match self {
            AdsConnection::Connected(client) => Some(client),
            AdsConnection::Disconnected => None,
        }
    }

    pub async fn handle_disconnect_error(&mut self, error: &Error) -> Result<()> {
        let should_disconnect = matches!(
            error,
            Error::Io(_, _) | Error::Ads(_, _, 0x006) | Error::Reply(_, "unexpected invoke ID", _)
        );

        if should_disconnect {
            warn!("PLC client error indicates we should disconnect...");

            self.disconnect().await?;
        }

        Ok(())
    }
}
