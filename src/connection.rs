use std::sync::Arc;

use tokio::net::ToSocketAddrs;
use tracing::info;

use crate::{core, AmsAddr, Result, Timeouts};

#[derive(Debug, Clone, Default)]
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

    pub fn client(&self) -> Option<Arc<core::Client>> {
        match self {
            AdsConnection::Connected(client) => Some(client.clone()),
            AdsConnection::Disconnected => None,
        }
    }
}
