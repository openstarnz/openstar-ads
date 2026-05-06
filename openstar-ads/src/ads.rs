use ads_client::{self as ads};
use std::{net::Ipv4Addr, sync::Arc, time::Duration};
use tokio::{net::ToSocketAddrs, sync::Mutex, time::sleep};
use tracing::{error, info, warn};

use crate::{
    AdsConnection, AdsData, AdsError, AdsParams, NotificationSubscription, Result, SymbolMap,
    SymbolTree, SymbolTypeTree,
};

pub use ads::{AdsTimeout, AmsAddr, AmsNetId, AmsPort};

pub struct AdsBuilder<RouterAddr> {
    client_builder: ads::ClientBuilder<RouterAddr>,
    set_to_run_mode: bool,
}

impl AdsBuilder<()> {
    pub fn new(dst_ams_addr: ads::AmsAddr) -> AdsBuilder<(Ipv4Addr, u16)> {
        AdsBuilder {
            client_builder: ads::ClientBuilder::new(dst_ams_addr),
            set_to_run_mode: Default::default(),
        }
    }
}

impl<RouterAddr> AdsBuilder<RouterAddr> {
    pub fn router_addr<NextRouterAddr: ToSocketAddrs>(
        self,
        router_addr: NextRouterAddr,
    ) -> AdsBuilder<NextRouterAddr> {
        AdsBuilder {
            client_builder: self.client_builder.router_addr(router_addr),
            set_to_run_mode: self.set_to_run_mode,
        }
    }
}

impl<RouterAddr> AdsBuilder<RouterAddr> {
    pub fn src_ams_addr(mut self, src_ams_addr: AmsAddr) -> Self {
        self.client_builder = self.client_builder.src_ams_addr(src_ams_addr);
        self
    }

    pub fn timeout(mut self, timeout: AdsTimeout) -> Self {
        self.client_builder = self.client_builder.timeout(timeout);
        self
    }

    pub fn retry_delay(mut self, retry_delay: Option<Duration>) -> Self {
        self.client_builder = self.client_builder.retry_delay(retry_delay);
        self
    }

    pub fn set_to_run_mode(mut self, set_to_run_mode: bool) -> Self {
        self.set_to_run_mode = set_to_run_mode;
        self
    }
}

impl<RouterAddr: ToSocketAddrs + Clone> AdsBuilder<RouterAddr> {
    pub fn build(self) -> Ads<RouterAddr> {
        Ads::new(self.client_builder, self.set_to_run_mode)
    }
}

pub struct Ads<RouterAddr> {
    client_builder: ads::ClientBuilder<RouterAddr>,
    set_to_run_mode: bool,
    connection: Arc<Mutex<AdsConnection>>,
}

/// Provides a more user friendly wrapper to interact with the OpenStar ADS PLC's.
impl<RouterAddr: ToSocketAddrs + Clone> Ads<RouterAddr> {
    pub fn new(client_builder: ads::ClientBuilder<RouterAddr>, set_to_run_mode: bool) -> Self {
        Self {
            client_builder,
            set_to_run_mode,
            connection: Default::default(),
        }
    }

    /// Blocks the current thread until a PLC is successfully connected over ADS.
    pub async fn run_connection_loop(&self) {
        loop {
            if !self.is_connected().await {
                let mut connection = self.connection.lock().await;
                if let Err(error) = connection
                    .connect(self.client_builder.clone(), self.set_to_run_mode)
                    .await
                {
                    warn!("PLC connection failed, {}. Retrying in 2 seconds...", error);
                } else {
                    info!("PLC connection successful!");

                    return;
                }
            }

            sleep(Duration::from_secs(2)).await;
        }
    }

    /// Disconnects the internal ADS client
    pub async fn disconnect(&self) -> Result<()> {
        let mut connection = self.connection.lock().await;
        connection.disconnect().await?;
        Ok(())
    }

    pub async fn is_connected(&self) -> bool {
        let connection = self.connection.lock().await;
        match *connection {
            AdsConnection::Connected(_) => true,
            AdsConnection::Disconnected => false,
        }
    }

    /// Read a symbol from the PLC.
    ///
    /// Returns None if the PLC is not connected.
    pub async fn read_symbol<T: AdsData>(&self, symbol: &str) -> Result<Option<T>> {
        let mut connection = self.connection.lock().await;

        let Some(client) = connection.client_mut() else {
            return Ok(None);
        };

        match client.read_symbol(symbol).await {
            Ok(value) => Ok(Some(value)),
            Err(error) => {
                error!("PLC client error when reading symbol {}: {}", symbol, error);

                connection.handle_disconnect_error(&error).await?;

                Err(error)
            }
        }
    }

    /// Gets a symbol type tree for a given symbol path.
    ///
    /// Returns None if the PLC is not connected.
    /// Returns any errors from the PLC
    pub async fn get_dynamic_type_tree(&self, symbol: &str) -> Result<Option<SymbolTypeTree>> {
        let mut connection = self.connection.lock().await;

        let Some(client) = connection.client_mut() else {
            return Ok(None);
        };

        match client.get_dynamic_type_tree(symbol).await {
            Ok(type_tree) => Ok(Some(type_tree)),
            Err(error) => {
                error!(
                    "ADS client error when getting information for symbol {}: {}",
                    symbol, error
                );

                match &error {
                    AdsError::SymbolTypeTree(_symbol_type_tree_error) => {
                        connection.disconnect().await?
                    }
                    error => connection.handle_disconnect_error(error).await?,
                };

                Err(error)
            }
        }
    }

    /// Calls an RPC method on the PLC.
    ///
    /// Returns None if the PLC is not connected.
    pub async fn invoke_rpc_method<Params: AdsParams>(
        &self,
        symbol: &str,
        params: Params,
    ) -> Result<Option<()>> {
        let mut connection = self.connection.lock().await;

        let Some(client) = connection.client_mut() else {
            return Ok(None);
        };

        if let Err(error) = client.invoke_rpc_method(symbol, params).await {
            error!(
                "PLC client error when invoking RPC method {}: {}",
                symbol, error
            );

            connection.handle_disconnect_error(&error).await?;

            return Err(error);
        };

        Ok(Some(()))
    }

    /// Calls an RPC method on the PLC that returns a value.
    ///
    /// Returns None if the PLC is not connected.
    pub async fn fetch_from_rpc_method<Params: AdsParams, Value: AdsData>(
        &self,
        symbol: &str,
        params: Params,
    ) -> Result<Option<Value>> {
        let mut connection = self.connection.lock().await;

        let Some(client) = connection.client_mut() else {
            return Ok(None);
        };

        match client.fetch_from_rpc_method(symbol, params).await {
            Ok(value) => Ok(Some(value)),
            Err(error) => {
                error!(
                    "PLC client error when invoking RPC method {}: {}",
                    symbol, error
                );

                connection.handle_disconnect_error(&error).await?;

                Err(error)
            }
        }
    }

    /// Subscribes to a notification channel on the PLC, returning a handle to the channel.
    ///
    /// Returns None if the PLC is not connected.
    pub async fn subscribe<T: AdsData + Send + Sync + 'static>(
        &self,
        symbol: &str,
    ) -> Result<Option<NotificationSubscription<T>>> {
        let mut connection = self.connection.lock().await;

        let Some(client) = connection.client_mut() else {
            return Ok(None);
        };

        match client.subscribe(symbol).await {
            Ok(subscription) => Ok(Some(subscription)),
            Err(error) => {
                error!(
                    "PLC client error when subscribing to notifications from {}: {}",
                    symbol, error
                );

                connection.handle_disconnect_error(&error).await?;

                Err(error)
            }
        }
    }

    /// Subscribes to the given symbol using the symbol type tree and sends deserialised tree-like data back with the sender channel.
    ///
    /// Returns None if the PLC is not connected.
    /// Returns any errors from the PLC.
    pub async fn subscribe_tree(
        &self,
        symbol: &str,
        symbol_type_tree: SymbolTypeTree,
    ) -> Result<Option<NotificationSubscription<SymbolTree>>> {
        let mut connection = self.connection.lock().await;

        let Some(client) = connection.client_mut() else {
            return Ok(None);
        };

        match client.subscribe_tree(symbol, symbol_type_tree).await {
            Ok(subscription) => Ok(Some(subscription)),
            Err(error) => {
                error!(
                    "PLC client error when subscribing to notifications from {}: {}",
                    symbol, error
                );

                connection.handle_disconnect_error(&error).await?;

                Err(error)
            }
        }
    }

    /// Subscribes to the given symbol using the symbol type tree and sends deserialised flattened map data back with the sender channel.
    /// The key to the map is the path of the symbol relative to the symbol provided.
    ///
    /// Returns None if the PLC is not connected.
    /// Returns any errors from the PLC.
    pub async fn subscribe_map(
        &self,
        symbol: &str,
        symbol_type_tree: SymbolTypeTree,
    ) -> Result<Option<NotificationSubscription<SymbolMap>>> {
        let mut connection = self.connection.lock().await;

        let Some(client) = connection.client_mut() else {
            return Ok(None);
        };

        match client.subscribe_map(symbol, symbol_type_tree).await {
            Ok(subscription) => Ok(Some(subscription)),
            Err(error) => {
                error!(
                    "PLC client error when subscribing to notifications from {}: {}",
                    symbol, error
                );

                connection.handle_disconnect_error(&error).await?;

                Err(error)
            }
        }
    }

    /// Unsubscribe from a notification subscription.
    pub async fn unsubscribe<T>(&self, subscription: NotificationSubscription<T>) -> Result<()> {
        let mut connection = self.connection.lock().await;

        let Some(client) = connection.client_mut() else {
            return Ok(());
        };

        client.unsubscribe(subscription).await
    }
}
