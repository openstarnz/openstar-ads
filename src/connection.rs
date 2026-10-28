use std::sync::Arc;

use tokio::net::ToSocketAddrs;
use tracing::{error, info, warn};

use crate::{core, AdsState, AmsAddr, Error, Result, Timeouts};

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
        should_set_to_run_mode: bool,
    ) -> Result<()> {
        match self {
            AdsConnection::Connected(_) => {
                info!("Attempted to connect to PLC but it is already connected!")
            }
            AdsConnection::Disconnected => {
                let client = core::Client::new(router, target, source, timeouts).await?;

                if !is_run_mode(client).await? {
                    if should_set_to_run_mode {
                        set_to_run_mode(client.clone()).await?;
                    } else {
                        self.disconnect().await;
                        return Err(Error::Other("PLC not in run mode".to_owned()));
                    }
                }

                *self = AdsConnection::Connected(client);
            }
        }

        Ok(())
    }

    pub async fn disconnect(&mut self) {
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
    }

    pub fn client(&self) -> Option<Arc<core::Client>> {
        match self {
            AdsConnection::Connected(client) => Some(client.clone()),
            AdsConnection::Disconnected => None,
        }
    }

    pub async fn with_client<Callback, Output>(
        &self,
        name: &str,
        callback: Callback,
    ) -> Result<Output>
    where
        Callback: AsyncFn(Arc<core::Client>) -> Result<Output> + Send + Sync + 'static,
    {
        let client = {
            let Some(client) = self.client() else {
                return Err(Error::Disconnected);
            };
            client
        };

        let result = callback(client).await;

        self.maybe_disconnect(result, name).await
    }

    fn should_disconnect(error: &Error) -> bool {
        matches!(
            error,
            Error::Io(_, _) | Error::Ads(_, _, 0x006) | Error::Reply(_, "unexpected invoke ID", _)
        )
    }

    async fn maybe_disconnect<Output>(
        &mut self,
        result: Result<Output>,
        name: &str,
    ) -> Result<Output> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                error!("{name} error: {error}");

                if Self::should_disconnect(&error) {
                    warn!("PLC client error indicates we should disconnect...");

                    // Ignore disconnect error
                    let _ = self.disconnect().await;
                }

                Err(error)
            }
        }
    }
}

// Returns if the connected ADS device is in run mode.
async fn is_run_mode(client: Arc<core::Client>) -> Result<bool> {
    let state_info = client.read_state().await?;
    Ok(state_info.ads_state == AdsState::Run)
}

/// Attempts to set the ADS device into run mode if it is not already in it.
async fn set_to_run_mode(client: Arc<core::Client>) -> Result<()> {
    let device_info = client.read_device_info().await?;
    info!("Device Info: {:?}", device_info);

    let state_info = client.read_state().await?;
    info!("Device State: {:?}", state_info);

    if state_info.ads_state != AdsState::Run {
        info!("Attempting to set PLC to run mode...");

        client
            .write_control(AdsState::Run, state_info.device_state)
            .await?;

        let state_info = client.read_state().await?;
        info!("Device State: {:?}", state_info);
        if state_info.ads_state != AdsState::Run {
            return Err(Error::Other("failed to set PLC to run mode!".to_owned()));
        }
    }

    Ok(())
}
