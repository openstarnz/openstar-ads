use ads_client as ads;
use std::time::Duration;

use crate::{PlcClient, PlcError, Result};

pub enum PlcConnection {
    Connected(PlcClient),
    Disconnected,
}

impl Default for PlcConnection {
    fn default() -> Self {
        Self::Disconnected
    }
}

impl PlcConnection {
    pub async fn connect(
        &mut self,
        addr: &str,
        port: u16,
        timeout: Option<ads::AdsTimeout>,
        retry_delay: Option<Duration>,
        set_to_run_mode: bool,
    ) -> Result<()> {
        match self {
            PlcConnection::Connected(_) => {
                println!("Attempted to connect to PLC but it is already connected!")
            }
            PlcConnection::Disconnected => {
                let mut ads_client_builder =
                    ads::ClientBuilder::new(addr, port).set_retry_delay(retry_delay);
                if let Some(timeout) = timeout {
                    ads_client_builder = ads_client_builder.set_timeout(timeout);
                }
                let ads_client = ads_client_builder.build().await?;
                let client = PlcClient::new(ads_client);

                if !client.is_run_mode().await? && set_to_run_mode {
                    client.set_to_run_mode()?;
                }

                if !client.is_run_mode().await? {
                    return Err(PlcError::Other(
                        "PLC not in run mode, stopping connection.".to_owned(),
                    ));
                }

                *self = PlcConnection::Connected(client);
            }
        }

        Ok(())
    }

    pub fn disconnect(&mut self) {
        match self {
            PlcConnection::Connected(plc_client) => {
                plc_client.unsubscribe_all();

                *self = PlcConnection::Disconnected;

                println!("PLC connection was dropped.");
            }
            PlcConnection::Disconnected => {
                // Already disconnected...
            }
        }
    }

    pub fn client(&self) -> Option<&PlcClient> {
        match self {
            PlcConnection::Connected(plc_client) => Some(plc_client),
            PlcConnection::Disconnected => None,
        }
    }

    pub fn client_mut(&mut self) -> Option<&mut PlcClient> {
        match self {
            PlcConnection::Connected(plc_client) => Some(plc_client),
            PlcConnection::Disconnected => None,
        }
    }

    pub fn handle_disconnect_error(&mut self, error: &PlcError) {
        // TODO(mw): We should have a think about this.
        let should_disconnect = matches!(
            error,
            PlcError::Io(_, _)
                | PlcError::Ads(ads::AdsError {
                    n_error: 0x006,
                    s_msg: _
                })
                | PlcError::Reply(_, "unexpected invoke ID", _) // TODO(mw): This is a hangover from the ads library, we should probably remove.
        );

        if should_disconnect {
            println!("PLC client error indicates we should disconnect...");

            self.disconnect();
        }
    }
}
