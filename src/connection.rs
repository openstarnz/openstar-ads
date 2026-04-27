use ads_client as ads;
use std::time::Duration;

use crate::{AdsClient, AdsError, Result};

pub enum AdsConnection {
    Connected(AdsClient),
    Disconnected,
}

impl Default for AdsConnection {
    fn default() -> Self {
        Self::Disconnected
    }
}

impl AdsConnection {
    pub async fn connect(
        &mut self,
        addr: &str,
        port: u16,
        timeout: Option<ads::AdsTimeout>,
        retry_delay: Option<Duration>,
        set_to_run_mode: bool,
    ) -> Result<()> {
        match self {
            AdsConnection::Connected(_) => {
                println!("Attempted to connect to PLC but it is already connected!")
            }
            AdsConnection::Disconnected => {
                let mut ads_client_builder =
                    ads::ClientBuilder::new(addr, port).set_retry_delay(retry_delay);
                if let Some(timeout) = timeout {
                    ads_client_builder = ads_client_builder.set_timeout(timeout);
                }
                let ads_client = ads_client_builder.build().await?;
                let client = AdsClient::new(ads_client);

                if !client.is_run_mode().await? && set_to_run_mode {
                    client.set_to_run_mode()?;
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

    pub fn disconnect(&mut self) {
        match self {
            AdsConnection::Connected(plc_client) => {
                plc_client.unsubscribe_all();

                *self = AdsConnection::Disconnected;

                println!("PLC connection was dropped.");
            }
            AdsConnection::Disconnected => {
                // Already disconnected...
            }
        }
    }

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

    pub fn handle_disconnect_error(&mut self, error: &AdsError) {
        // TODO(mw): We should have a think about this.
        let should_disconnect = matches!(
            error,
            AdsError::Io(_, _)
                | AdsError::Ads(ads::AdsError {
                    n_error: 0x006,
                    s_msg: _
                })
                | AdsError::Reply(_, "unexpected invoke ID", _) // TODO(mw): This is a hangover from the ads library, we should probably remove.
        );

        if should_disconnect {
            println!("PLC client error indicates we should disconnect...");

            self.disconnect();
        }
    }
}
