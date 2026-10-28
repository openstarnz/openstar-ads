use std::sync::Arc;

use tokio::net::ToSocketAddrs;
use tracing::{error, info, warn};

use crate::{core, AdsState, AmsAddr, Error, Result, Timeouts};

#[derive(Debug, Clone)]
pub struct AdsConnection {
    client: Option<Arc<core::Client>>,
    is_connected: bool,
}

impl Default for AdsConnection {
    fn default() -> Self {
        Self {
            client: None,
            is_connected: false,
        }
    }
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
        if self.is_connected {
            info!("Attempted to connect to PLC but it is already connected!");
            return Ok(());
        };

        let client = core::Client::new(router, target, source, timeouts).await?;
        self.client = Some(client);

        if !self.is_run_mode().await? {
            if should_set_to_run_mode {
                self.set_to_run_mode().await?;
            } else {
                self.disconnect().await;
                return Err(Error::Other("PLC not in run mode".to_owned()));
            }
        }

        self.is_connected = true;

        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    pub async fn disconnect(&mut self) {
        if let Some(ref client) = self.client {
            client.close().await;

            info!("PLC connection was dropped.");
        }

        self.is_connected = false;
    }

    pub async fn with_client<Callback, Output>(
        &mut self,
        name: &str,
        callback: Callback,
    ) -> Result<Output>
    where
        Callback: AsyncFn(Arc<core::Client>) -> Result<Output> + Send + Sync + 'static,
    {
        let Some(ref client) = self.client else {
            return Err(Error::Disconnected);
        };

        let result = callback(client.clone()).await;
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

    fn should_disconnect(error: &Error) -> bool {
        matches!(
            error,
            Error::Io(_, _) | Error::Ads(_, _, 0x006) | Error::Reply(_, "unexpected invoke ID", _)
        )
    }

    /// Returns if the connected ADS device is in run mode.
    async fn is_run_mode(&mut self) -> Result<bool> {
        self.with_client("is_run_mode", async move |client| {
            let state_info = client.read_state().await?;
            Ok(state_info.ads_state == AdsState::Run)
        })
        .await
    }

    /// Attempts to set the ADS device into run mode if it is not already in it.
    async fn set_to_run_mode(&mut self) -> Result<()> {
        self.with_client("set_to_run_mode", async move |client| {
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
        })
        .await
    }
}
