use ads_client as ads;
use tokio::net::ToSocketAddrs;
use tracing::{error, info, warn};

use crate::{AdsClient, AdsError, Result};

#[derive(Default)]
pub enum AdsConnection {
    Connected(AdsClient),
    #[default]
    Disconnected,
}

impl AdsConnection {
    pub async fn connect<RouterAddr: ToSocketAddrs>(
        &mut self,
        client_builder: ads::ClientBuilder<RouterAddr>,
        set_to_run_mode: bool,
    ) -> Result<()> {
        match self {
            AdsConnection::Connected(_) => {
                info!("Attempted to connect to PLC but it is already connected!")
            }
            AdsConnection::Disconnected => {
                let ads_client = client_builder.build().await?;
                let client = AdsClient::new(ads_client);

                if !client.is_run_mode().await? && set_to_run_mode {
                    client.set_to_run_mode().await?;
                }

                if !client.is_run_mode().await? {
                    return Err(AdsError::Other(
                        "PLC not in run mode, stopping connection.".to_owned(),
                    ));
                }

                *self = AdsConnection::Connected(client);
            }
        }

        Ok(())
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        match self {
            AdsConnection::Connected(plc_client) => {
                if let Err(error) = plc_client.unsubscribe_all().await {
                    error!("Error unsubscribing on disconnect: {error}")
                };

                *self = AdsConnection::Disconnected;

                info!("PLC connection was dropped.");
            }
            AdsConnection::Disconnected => {
                // Already disconnected...
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn client(&self) -> Option<&AdsClient> {
        match self {
            AdsConnection::Connected(plc_client) => Some(plc_client),
            AdsConnection::Disconnected => None,
        }
    }

    pub fn client_mut(&mut self) -> Option<&mut AdsClient> {
        match self {
            AdsConnection::Connected(plc_client) => Some(plc_client),
            AdsConnection::Disconnected => None,
        }
    }

    pub async fn handle_disconnect_error(&mut self, error: &AdsError) -> Result<()> {
        // TODO(mw): We should have a think about this.
        let should_disconnect = matches!(
            error,
            AdsError::Io(_, _)
                | AdsError::Ads(ads::AdsError {
                    n_error: 0x006,
                    s_msg: _
                })
        );

        if should_disconnect {
            warn!("PLC client error indicates we should disconnect...");

            self.disconnect().await?;
        }

        Ok(())
    }
}
