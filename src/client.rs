use bytes::Bytes;
use std::{collections::HashMap, fmt::Debug, time::Duration};
use tracing::{error, info};

use crate::{
    core::{self, index, NotificationAttributes, NotificationTransmissionMode},
    get_symbol_info, AdsData, AdsParams, AdsState, Result, SymbolMap, SymbolMapExt, SymbolTree,
    SymbolTypeMap, SymbolTypeMapExt, SymbolTypeTree,
};

#[derive(Debug, Copy, Clone)]
pub struct SymbolHandle(u32);

impl SymbolHandle {
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

pub struct NotificationSubscription<'a, T> {
    from_bytes: Box<dyn Fn(Bytes) -> T + Send + Sync + 'static>,
    core_subscription: core::NotificationSubscription<'a>,
}

impl<T> Debug for NotificationSubscription<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificaionSubscription")
            .field("core_subscription", &self.core_subscription)
            .finish_non_exhaustive()
    }
}

impl<'a, T> NotificationSubscription<'a, T> {
    pub async fn recv(&mut self) -> Option<T> {
        self.core_subscription
            .recv()
            .await
            .map(|bytes| (self.from_bytes)(bytes))
    }
}

pub struct AdsClient {
    core_client: core::Client,
    symbol_handles: HashMap<String, SymbolHandle>,
}

/// Provides a more user friendly wrapper to interact with the OpenStar PLC's.
impl AdsClient {
    pub fn new(core_client: core::Client) -> Self {
        Self {
            core_client,
            symbol_handles: Default::default(),
        }
    }

    async fn get_symbol_handle(&self, symbol: &str) -> Result<SymbolHandle> {
        let mut read_data = [0; 4];
        let write_data = symbol.as_bytes();

        self.core_client
            .read_write(index::GET_SYMHANDLE_BYNAME, 0, &mut read_data, write_data)
            .await?;

        Ok(SymbolHandle(u32::from_le_bytes(read_data)))
    }

    async fn symbol_handle(&mut self, symbol: &str) -> Result<SymbolHandle> {
        let handle = self.symbol_handles.get(symbol);
        let handle = match handle {
            Some(handle) => handle.to_owned(),
            None => {
                let handle = self.get_symbol_handle(symbol).await?;
                self.symbol_handles.insert(symbol.to_owned(), handle);
                handle
            }
        };

        Ok(handle)
    }

    /// Returns if the connected ADS device is in run mode.
    pub async fn is_run_mode(&self) -> Result<bool> {
        let state_info = self.core_client.read_state().await?;
        Ok(state_info.ads_state == AdsState::Run)
    }

    /// Attempts to set the ADS device into run mode if it is not already in it.
    pub async fn set_to_run_mode(&self) -> Result<()> {
        let device_info = self.core_client.read_device_info().await?;
        info!("Device Info: {:?}", device_info);

        let state_info = self.core_client.read_state().await?;
        info!("Device State: {:?}", state_info);

        if state_info.ads_state != AdsState::Run {
            info!("Attempting to set PLC to run mode...");

            self.core_client
                .write_control(AdsState::Run, state_info.device_state)
                .await?;

            let state_info = self.core_client.read_state().await?;
            info!("Device State: {:?}", state_info);
            assert_eq!(state_info.ads_state, AdsState::Run);
        }

        Ok(())
    }

    /// Read the value of a symbol with a given type once.
    pub async fn read_symbol<T: AdsData>(&mut self, symbol: &str) -> Result<T> {
        let mut read_data = T::default();

        let index_offset = self.symbol_handle(symbol).await?.as_u32();

        self.core_client
            .read_exact(
                index::RW_SYMVAL_BYHANDLE,
                index_offset,
                read_data.as_mut_bytes(),
            )
            .await?;

        Ok(read_data)
    }

    /// Gets a type tree for the symbol to get the format of a symbol's internal structure at runtime.
    pub async fn get_dynamic_type_tree(&self, symbol: &str) -> Result<SymbolTypeTree> {
        let symbol_name = symbol;
        let (symbols, type_map) = get_symbol_info(&self.core_client).await?;
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
    }

    /// Invokes a PLC method (which has the attribute 'TcRpcEnable') with parameters.
    pub async fn invoke_rpc_method<Params: AdsParams>(
        &mut self,
        symbol: &str,
        params: Params,
    ) -> Result<()> {
        let index_offset = self.symbol_handle(symbol).await?.as_u32();
        let write_data = params.into_data();

        self.core_client
            .read_write_exact(
                index::RW_SYMVAL_BYHANDLE,
                index_offset,
                &mut [],
                &write_data,
            )
            .await?;

        Ok(())
    }

    /// Invokes a PLC method (which has the attribute 'TcRpcEnable') and returns the resulting value.
    pub async fn fetch_from_rpc_method<Params: AdsParams, Value: AdsData>(
        &mut self,
        symbol: &str,
        params: Params,
    ) -> Result<Value> {
        let index_offset = self.symbol_handle(symbol).await?.as_u32();
        let mut read_data = Value::default();
        let write_data = params.into_data();

        self.core_client
            .read_write_exact(
                index::RW_SYMVAL_BYHANDLE,
                index_offset,
                read_data.as_mut_bytes(),
                &write_data,
            )
            .await?;

        Ok(read_data)
    }

    /// Subscribes to a symbol.
    pub async fn subscribe<T: AdsData + Send + Sync + 'static>(
        &mut self,
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
    pub async fn subscribe_tree(
        &mut self,
        symbol: &str,
        symbol_type_tree: SymbolTypeTree,
    ) -> Result<NotificationSubscription<SymbolTree>> {
        let size = symbol_type_tree.get_size();
        let from_bytes = move |payload| SymbolTree::from_bytes(payload, &symbol_type_tree, 0);
        self.subscribe_inner(symbol, size, from_bytes).await
    }

    /// Subscribes to a symbol map using the symbol type tree.
    pub async fn subscribe_map(
        &mut self,
        symbol: &str,
        symbol_type_tree: SymbolTypeTree,
    ) -> Result<NotificationSubscription<SymbolMap>> {
        let size = symbol_type_tree.get_size();
        let symbol_type_map = SymbolTypeMap::from_tree(symbol_type_tree, 0, String::new());
        let from_bytes = move |payload| SymbolMap::from_bytes(payload, &symbol_type_map);
        self.subscribe_inner(symbol, size, from_bytes).await
    }

    async fn subscribe_inner<T: Send + Sync + 'static>(
        &mut self,
        symbol: &str,
        size: usize,
        from_bytes: impl Fn(Bytes) -> T + Send + Sync + 'static,
    ) -> Result<NotificationSubscription<T>> {
        let index_offset = self.symbol_handle(symbol).await?.as_u32();

        let attributes = NotificationAttributes {
            length: size,
            trans_mode: NotificationTransmissionMode::ServerOnChange,
            max_delay: Duration::ZERO,
            // TODO: setting this to higher e.g: 1000ms does not work, maybe because the status data is changing every PLC cycle?
            // NB: Setting this to 10ms to match the PLC cycle time that it seems to be reporting at anyway
            cycle_time: Duration::from_millis(10),
        };

        let core_subscription = self
            .core_client
            .add_notification(index::RW_SYMVAL_BYHANDLE, index_offset, &attributes)
            .await?;

        let subscription = NotificationSubscription {
            core_subscription,
            from_bytes: Box::new(from_bytes),
        };

        Ok(subscription)
    }
}
