use ads_client as ads;
use bon::bon;
use crossbeam_channel::Receiver;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

use crate::{
    AdsConnection, AdsData, AdsError, AdsParams, NotificationSubscription, Result, SymbolTypeTree,
    SymbolTypeTreeError,
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
    async fn disconnect(&self) {
        let mut connection = self.connection.lock().await;
        connection.disconnect().await;
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

                connection.handle_disconnect_error(&error).await;

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
                        connection.disconnect().await
                    }
                    error => connection.handle_disconnect_error(error).await,
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

            connection.handle_disconnect_error(&error);

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

                connection.handle_disconnect_error(&error);

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

        match client.subscribe::<T>(symbol).await {
            Ok(subscription) => Ok(Some(subscription)),
            Err(error) => {
                eprintln!(
                    "PLC client error when subscribing to notifications from {}: {}",
                    symbol, error
                );

                connection.handle_disconnect_error(&error);

                Err(error)
            }
        }
    }

    /// Subscribes to the given symbol using the symbol type tree and sends deserialised tree-like data back with the sender channel.
    ///
    /// Returns None if the PLC is not connected.
    /// Returns any errors from the PLC.
    pub async fn start_dynamic_symbol_receiver(
        &self,
        symbol: String,
        symbol_type_tree: SymbolTypeTree,
        sender_channel: broadcast::Sender<SymbolTree>,
    ) -> Result<Option<()>> {
        let mut notif_handle = None;
        {
            let mut connection = self.connection.lock().await;

            if let Some(client) = connection.client_mut() {
                notif_handle = Some(
                    client
                        .add_dynamic_symbol_notification(&symbol, &symbol_type_tree)
                        .map_err(|error| {
                            println!(
                                "PLC client error when getting information for symbol {}: {}",
                                symbol, error
                            );

                            connection.handle_disconnect_error(&error);

                            error
                        })?,
                );
            }
        }

        let Some(mut notif_receiver) = self.notification_receiver() else {
            return Ok(None);
        };

        let Some(notif_handle) = notif_handle else {
            return Ok(None);
        };

        // Runs the actual notification loop which filters for the correct symbol handle.
        loop {
            let resp =
                tokio::task::spawn_blocking(move || Self::recv_blocking(notif_receiver)).await.context("Tokio spawn blocking function failed while trying to receive from PLC channel.")?;
            notif_receiver = resp.0;
            let notif_result = resp.1;
            let notif = notif_result
                .context("Receive error while receiving from PLC notification channel.")?;
            for sample in notif.samples() {
                if sample.handle == notif_handle {
                    let symbol_tree: SymbolTree = (&symbol_type_tree, sample.data, 0).into();
                    // If there is an error sending it means that all receivers are gone and therefore this thread has successfully ended.
                    if let Err(_err) = sender_channel.send(symbol_tree) {
                        return Ok(Some(()));
                    };
                }
            }
        }
    }

    /// Subscribes to the given symbol using the symbol type tree and sends deserialised flattened map data back with the sender channel.
    /// The key to the map is the path of the symbol relative to the symbold symbol provided.
    ///
    /// Returns None if the PLC is not connected.
    /// Returns any errors from the PLC.
    pub async fn start_dynamic_symbol_map_receiver(
        &self,
        symbol: String,
        symbol_type_tree: SymbolTypeTree,
        sender_channel: broadcast::Sender<SymbolMap>,
    ) -> Result<Option<()>> {
        let mut notif_handle = None;
        {
            let mut connection = self.connection.lock().await;

            if let Some(client) = connection.client_mut() {
                notif_handle = Some(
                    client
                        .add_dynamic_symbol_notification(&symbol, &symbol_type_tree)
                        .map_err(|error| {
                            println!(
                                "PLC client error when getting information for symbol {}: {}",
                                symbol, error
                            );

                            connection.handle_disconnect_error(&error);

                            error
                        })?,
                );
            }
        }

        let Some(mut notif_receiver) = self.notification_receiver() else {
            return Ok(None);
        };

        let Some(notif_handle) = notif_handle else {
            return Ok(None);
        };

        let symbol_type_map = SymbolTypeMap::from_tree(symbol_type_tree, 0, "".to_string());
        // Runs the actual notification loop which filters for the correct symbol handle.
        loop {
            let resp =
                tokio::task::spawn_blocking(move || Self::recv_blocking(notif_receiver)).await.context("Tokio spawn blocking function failed while trying to receive from PLC channel.")?;
            notif_receiver = resp.0;
            let notif_result = resp.1;
            let notif = notif_result
                .context("Receive error while receiving from PLC notification channel.")?;
            for sample in notif.samples() {
                if sample.handle == notif_handle {
                    let symbol_tree: SymbolMap =
                        SymbolMap::from_bytes(&symbol_type_map, sample.data);
                    // If there is an error sending it means that all receivers are gone and therefore this thread has successfully ended.
                    if let Err(_err) = sender_channel.send(symbol_tree) {
                        return Ok(Some(()));
                    };
                }
            }
        }
    }

    // Fully takes ownership of the receiver so that this can be run within a spawn_blocking enclosure.
    // Returns out the channel again with the result so that the caller can ensure the channel can be used again by awaiting the response.
    fn recv_blocking<T>(channel: Receiver<T>) -> (Receiver<T>, Result<T>) {
        let result = channel.recv_timeout(std::time::Duration::from_secs(5));
        let value = match result {
            Ok(value) => anyhow::Result::Ok(value),
            Err(err) => match err {
                crossbeam_channel::RecvTimeoutError::Timeout => {
                    anyhow::Result::Err(anyhow!("Message took more than 5 seconds to receive."))
                }
                crossbeam_channel::RecvTimeoutError::Disconnected => {
                    anyhow::Result::Err(anyhow!("PLC channel closed."))
                }
            },
        };
        (channel, value)
    }
}
