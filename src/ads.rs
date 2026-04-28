use ads_client as ads;
use bon::bon;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

use crate::{
    AdsConnection, AdsData, AdsError, AdsParams, NotificationSubscription, Result, SymbolMap,
    SymbolTree, SymbolTypeTree,
};

pub struct Ads {
    addr: String,
    port: u16,
    timeout: Option<Timeout>,
    retry_delay: Option<Duration>,
    set_to_run_mode: bool,
    connection: Arc<Mutex<AdsConnection>>,
}

// Note(mw): We need our own Timeout enum, only because
//   ads_client::AdsTimeout doesn't implement Clone.
#[derive(Debug, Clone)]
pub enum Timeout {
    /// Default timeout is 5 seconds
    DefaultTimeout,
    /// Custom timeout (in seconds)
    CustomTimeout(u64),
}

impl From<Timeout> for ads::AdsTimeout {
    fn from(value: Timeout) -> Self {
        match value {
            Timeout::DefaultTimeout => ads::AdsTimeout::DefaultTimeout,
            Timeout::CustomTimeout(timeout) => ads::AdsTimeout::CustomTimeout(timeout),
        }
    }
}

/// Provides a more user friendly wrapper to interact with the OpenStar ADS PLC's.
#[bon]
impl Ads {
    #[builder]
    pub fn new(
        addr: String,
        port: u16,
        timeout: Option<Timeout>,
        retry_delay: Option<Duration>,
        set_to_run_mode: bool,
    ) -> Self {
        Self {
            addr,
            port,
            timeout,
            retry_delay,
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
                    .connect(
                        &self.addr,
                        self.port,
                        self.timeout.clone().map(Into::into),
                        self.retry_delay,
                        self.set_to_run_mode,
                    )
                    .await
                {
                    println!("PLC connection failed, {}. Retrying in 2 seconds...", error);
                } else {
                    println!("PLC connection successful!");
                }
            }

            std::thread::sleep(Duration::from_secs(2));
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
                println!("PLC client error when reading symbol {}: {}", symbol, error);

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
                println!(
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
            eprintln!(
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
                eprintln!(
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
                eprintln!(
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
                eprintln!(
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
                eprintln!(
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
