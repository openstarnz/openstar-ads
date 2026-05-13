use bytes::Bytes;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::{collections::HashMap, fmt::Debug, time::Duration};
use tokio::sync::mpsc;
use tokio::{net::ToSocketAddrs, sync::Mutex, time::sleep};
use tracing::{error, info, warn};

use crate::{
    core::{self, index, NotificationAttributes, NotificationTransmissionMode},
    get_symbol_info, AdsConnection, AdsData, AdsParams, AdsState, AmsAddr, Error, Result,
    SymbolMap, SymbolMapExt, SymbolTree, SymbolTypeMap, SymbolTypeMapExt, SymbolTypeTree, Timeouts,
};

#[derive(Debug, Clone)]
pub struct AdsBuilder<RouterAddr> {
    router: RouterAddr,
    target: AmsAddr,
    source: Option<AmsAddr>,
    timeouts: Timeouts,
    set_to_run_mode: bool,
}

impl AdsBuilder<()> {
    pub fn new(target: AmsAddr) -> AdsBuilder<(Ipv4Addr, u16)> {
        AdsBuilder {
            router: (Ipv4Addr::new(127, 0, 0, 1), 48898),
            target,
            source: Default::default(),
            timeouts: Default::default(),
            set_to_run_mode: Default::default(),
        }
    }
}

impl<RouterAddr> AdsBuilder<RouterAddr> {
    pub fn router<NextRouterAddr: ToSocketAddrs>(
        self,
        router: NextRouterAddr,
    ) -> AdsBuilder<NextRouterAddr> {
        AdsBuilder {
            router,
            target: self.target,
            source: self.source,
            timeouts: self.timeouts,
            set_to_run_mode: self.set_to_run_mode,
        }
    }
}

impl<RouterAddr> AdsBuilder<RouterAddr> {
    pub fn source(mut self, source: AmsAddr) -> Self {
        self.source = Some(source);
        self
    }

    pub fn timeouts(mut self, timeouts: Timeouts) -> Self {
        self.timeouts = timeouts;
        self
    }
}

impl<RouterAddr: ToSocketAddrs + Clone> AdsBuilder<RouterAddr> {
    pub fn build(self) -> Ads<RouterAddr> {
        Ads::new(
            self.router,
            self.target,
            self.source,
            self.timeouts,
            self.set_to_run_mode,
        )
    }
}

/// Provides a more user friendly wrapper to interact with the OpenStar ADS PLC's.
#[derive(Debug)]
pub struct Ads<RouterAddr> {
    router: RouterAddr,
    target: AmsAddr,
    source: Option<AmsAddr>,
    timeouts: Timeouts,
    set_to_run_mode: bool,
    connection: Arc<Mutex<AdsConnection>>,
    symbol_handles: Arc<Mutex<HashMap<String, SymbolHandle>>>,
}

impl<RouterAddr: ToSocketAddrs + Clone> Ads<RouterAddr> {
    pub fn new(
        router: RouterAddr,
        target: AmsAddr,
        source: Option<AmsAddr>,
        timeouts: Timeouts,
        set_to_run_mode: bool,
    ) -> Self {
        Self {
            router,
            target,
            source,
            timeouts,
            set_to_run_mode,
            connection: Default::default(),
            symbol_handles: Default::default(),
        }
    }

    /// Connect to a PLC over ADS, retry on failure.
    pub async fn connect(&self) -> Result<()> {
        {
            let mut connection = self.connection.lock().await;
            connection
                .connect(self.router.clone(), self.target, self.source, self.timeouts)
                .await?;
        }

        if !self.is_run_mode().await? {
            if self.set_to_run_mode {
                self.set_to_run_mode().await?;
            } else {
                return Err(Error::Other("PLC not in run mode, stopping connection."));
            }
        }

        Ok(())
    }

    /// Connect to a PLC over ADS, retry on failure.
    pub async fn connect_with_retry(&self) {
        loop {
            if !self.is_connected().await {
                if let Err(error) = self.connect().await {
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
        {
            let mut symbol_handles = self.symbol_handles.lock().await;
            symbol_handles.clear()
        }

        {
            let mut connection = self.connection.lock().await;
            connection.disconnect().await?;
        }

        Ok(())
    }

    pub async fn is_connected(&self) -> bool {
        let connection = self.connection.lock().await;
        match *connection {
            AdsConnection::Connected(_) => true,
            AdsConnection::Disconnected => false,
        }
    }

    async fn get_symbol_handle(&self, symbol: &str) -> Result<SymbolHandle> {
        let mut read_data = [0; 4];
        let write_data = symbol.as_bytes();

        self.with_client("get_symbol_handle", async move |client| {
            client
                .read_write(index::GET_SYMHANDLE_BYNAME, 0, &mut read_data, write_data)
                .await?;

            Ok(u32::from_le_bytes(read_data))
        })
        .await
    }

    async fn symbol_handle(&self, symbol: &str) -> Result<SymbolHandle> {
        let mut symbol_handles = self.symbol_handles.lock().await;
        let handle = symbol_handles.get(symbol);
        let handle = match handle {
            Some(handle) => handle.to_owned(),
            None => {
                let handle = self.get_symbol_handle(symbol).await?;
                symbol_handles.insert(symbol.to_owned(), handle);
                handle
            }
        };

        Ok(handle)
    }

    /// Returns if the connected ADS device is in run mode.
    pub async fn is_run_mode(&self) -> Result<bool> {
        self.with_client("is_run_mode", async move |client| {
            let state_info = client.read_state().await?;
            Ok(state_info.ads_state == AdsState::Run)
        })
        .await
    }

    /// Attempts to set the ADS device into run mode if it is not already in it.
    pub async fn set_to_run_mode(&self) -> Result<()> {
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
                assert_eq!(state_info.ads_state, AdsState::Run);
            }

            Ok(())
        })
        .await
    }

    /// Read the value of a symbol with a given type once.
    pub async fn read_symbol<T: AdsData>(&self, symbol: &str) -> Result<T> {
        let index_offset = self.symbol_handle(symbol).await?;

        self.with_client("read_symbol", async move |client| {
            let mut read_data = T::default();

            client
                .read_exact(
                    index::RW_SYMVAL_BYHANDLE,
                    index_offset,
                    read_data.as_mut_bytes(),
                )
                .await?;

            Ok(read_data)
        })
        .await
    }

    /// Gets a type tree for the symbol to get the format of a symbol's internal structure at runtime.
    pub async fn get_dynamic_type_tree(&self, symbol: &str) -> Result<SymbolTypeTree> {
        self.with_client("get_dynamic_type_tree", async move |client| {
            let symbol_name = symbol;
            let (symbols, type_map) = get_symbol_info(&client).await?;
            let mut symbol = None;

            for symbol_info in symbols {
                if symbol_info.name.to_lowercase() == symbol_name.to_lowercase() {
                    symbol = Some(symbol_info);
                }
            }

            let Some(symbol) = symbol else {
                return Ok(SymbolTypeTree::Missing);
            };

            let symbol_type_tree = match SymbolTypeTree::from_symbol(&symbol, &type_map) {
                Ok(symbol_type_tree) => symbol_type_tree,
                Err(err) => {
                    error!("Error when getting symbol type from type map {err:?}");
                    SymbolTypeTree::Unknown(symbol.size)
                }
            };

            Ok(symbol_type_tree)
        })
        .await
    }

    /// Invokes a PLC method (which has the attribute 'TcRpcEnable') with parameters.
    pub async fn invoke_rpc_method<Params: AdsParams>(
        &self,
        symbol: &str,
        params: Params,
    ) -> Result<()> {
        let index_offset = self.symbol_handle(symbol).await?;
        let write_data = params.into_data();

        self.with_client("invoke_rpc_method", async move |client| {
            client
                .read_write_exact(
                    index::RW_SYMVAL_BYHANDLE,
                    index_offset,
                    &mut [],
                    &write_data,
                )
                .await?;

            Ok(())
        })
        .await
    }

    /// Invokes a PLC method (which has the attribute 'TcRpcEnable') with parameters and returns the resulting value.
    pub async fn fetch_from_rpc_method<Params: AdsParams, Value: AdsData>(
        &self,
        symbol: &str,
        params: Params,
    ) -> Result<Value> {
        let index_offset = self.symbol_handle(symbol).await?;
        let write_data = params.into_data();

        self.with_client("fetch_from_rpc_method", async move |client| {
            let mut read_data = Value::default();

            client
                .read_write_exact(
                    index::RW_SYMVAL_BYHANDLE,
                    index_offset,
                    read_data.as_mut_bytes(),
                    &write_data,
                )
                .await?;

            Ok(read_data)
        })
        .await
    }

    /// Subscribes to a symbol.
    pub async fn subscribe<T: AdsData + Send + Sync + 'static>(
        &self,
        symbol: &str,
    ) -> Result<NotificationSubscription<T>> {
        let size = T::size();
        let from_bytes = |payload| {
            // TODO(mw): Do we need to handle this failure better?
            T::from_bytes(payload).expect("Failed to parse PlcDataType from notification bytes")
        };
        self.subscribe_inner(symbol, size, from_bytes).await
    }

    /// Subscribes to a symbol tree using the symbol type tree.
    ///
    /// Sends deserialised tree-like data back with the sender channel.
    pub async fn subscribe_tree(
        &self,
        symbol: &str,
        symbol_type_tree: SymbolTypeTree,
    ) -> Result<NotificationSubscription<SymbolTree>> {
        let size = symbol_type_tree.get_size();
        let from_bytes = move |payload| SymbolTree::from_bytes(payload, &symbol_type_tree, 0);
        self.subscribe_inner(symbol, size, from_bytes).await
    }

    /// Subscribes to the given symbol using the symbol type map.
    ///
    /// Sends deserialised flattened map data back with the sender channel.
    /// The key to the map is the path of the symbol relative to the symbol provided.
    pub async fn subscribe_map(
        &self,
        symbol: &str,
        symbol_type_tree: SymbolTypeTree,
    ) -> Result<NotificationSubscription<SymbolMap>> {
        let size = symbol_type_tree.get_size();
        let symbol_type_map = SymbolTypeMap::from_tree(symbol_type_tree, 0, String::new());
        let from_bytes = move |payload| SymbolMap::from_bytes(payload, &symbol_type_map);
        self.subscribe_inner(symbol, size, from_bytes).await
    }

    async fn subscribe_inner<T: Send + Sync + 'static>(
        &self,
        symbol: &str,
        size: usize,
        from_bytes: impl Fn(Bytes) -> T + Send + Sync + 'static,
    ) -> Result<NotificationSubscription<T>> {
        let index_offset = self.symbol_handle(symbol).await?;

        let attributes = NotificationAttributes {
            length: size,
            trans_mode: NotificationTransmissionMode::ServerOnChange,
            max_delay: Duration::ZERO,
            // TODO: setting this to higher e.g: 1000ms does not work, maybe because the status data is changing every PLC cycle?
            // NB: Setting this to 10ms to match the PLC cycle time that it seems to be reporting at anyway
            cycle_time: Duration::from_millis(10),
        };

        let (receiver, _handle) = self
            .with_client("subscribe", async move |client| {
                client
                    .add_notification(index::RW_SYMVAL_BYHANDLE, index_offset, &attributes)
                    .await
            })
            .await?;

        let subscription = NotificationSubscription {
            receiver,
            from_bytes: Box::new(from_bytes),
        };

        Ok(subscription)
    }

    fn should_disconnect(error: &Error) -> bool {
        matches!(
            error,
            Error::Io(_, _) | Error::Ads(_, _, 0x006) | Error::Reply(_, "unexpected invoke ID", _)
        )
    }

    async fn with_client<Callback, Output>(
        &self,
        name: &str,
        mut callback: Callback,
    ) -> Result<Output>
    where
        Callback: AsyncFnMut(Arc<core::Client>) -> Result<Output>,
    {
        let client = {
            let connection = self.connection.lock().await;
            let Some(client) = connection.client() else {
                return Err(Error::Disconnected);
            };
            client
        };

        let result = callback(client).await;

        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                error!("{name} error: {error}");

                if Self::should_disconnect(&error) {
                    warn!("PLC client error indicates we should disconnect...");

                    self.disconnect().await?;
                }

                Err(error)
            }
        }
    }
}

pub type SymbolHandle = u32;

pub struct NotificationSubscription<T> {
    from_bytes: Box<dyn Fn(Bytes) -> T + Send + Sync + 'static>,
    receiver: mpsc::Receiver<Bytes>,
}

impl<T> Debug for NotificationSubscription<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationSubscription")
            .field("receiver", &self.receiver)
            .finish_non_exhaustive()
    }
}

impl<T> NotificationSubscription<T> {
    pub async fn recv(&mut self) -> Option<T> {
        self.receiver
            .recv()
            .await
            .map(|bytes| (self.from_bytes)(bytes))
    }
}
