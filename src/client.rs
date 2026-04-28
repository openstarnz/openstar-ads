use ads_client::{self as ads, AdsNotificationAttrib, StateInfo};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::mpsc;

use crate::{
    get_symbol_info, AdsData, AdsError, AdsParams, Result, Symbol, SymbolTypeTree, TypeMap,
};

#[derive(Debug, Copy, Clone)]
pub struct SymbolHandle(u32);

impl SymbolHandle {
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct NotificationHandle {
    is_cancelled: Arc<AtomicBool>,
    ads_handle: u32,
}

impl NotificationHandle {
    fn new(ads_handle: u32) -> Self {
        Self {
            is_cancelled: Arc::new(AtomicBool::new(false)),
            ads_handle,
        }
    }

    fn cancel(&self) {
        self.is_cancelled.store(true, Ordering::Relaxed);
    }

    fn is_cancelled(&self) -> bool {
        self.is_cancelled.load(Ordering::Relaxed)
    }
}

#[derive(Debug)]
pub struct NotificationSubscription<T> {
    receiver: mpsc::UnboundedReceiver<T>,
    handle: NotificationHandle,
}

impl<T> NotificationSubscription<T> {
    fn cancel(&self) {
        self.handle.cancel();
    }

    async fn recv(&mut self) -> Option<T> {
        if self.handle.is_cancelled() {
            return None;
        };
        self.receiver.recv().await
    }
}

pub struct AdsClient {
    ads_client: ads::Client,
    symbol_handles: HashMap<String, SymbolHandle>,
    notification_handles: Vec<NotificationHandle>,
}

/// Get u32 handle to the name in the write data. Index offset is 0.
const GET_SYMHANDLE_BYNAME: u32 = 0xF003;
const GET_SYMHANDLE_BYNAME_LEN: usize = 4;

/// Read/write data for a symbol by handle.
/// Use the handle as the index offset.
const RW_SYMVAL_BYHANDLE: u32 = 0xF005;

/// Provides a more user friendly wrapper to interact with the OpenStar PLC's.
impl AdsClient {
    pub fn new(ads_client: ads::Client) -> Self {
        Self {
            ads_client,
            symbol_handles: Default::default(),
            notification_handles: Default::default(),
        }
    }

    async fn get_symbol_handle(&self, symbol: &str) -> Result<SymbolHandle> {
        let mut read_data = [0; GET_SYMHANDLE_BYNAME_LEN];
        let write_data = symbol.as_bytes();

        self.ads_client
            .read_write(GET_SYMHANDLE_BYNAME, 0, &mut read_data, write_data)
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

    async fn get_symbol_info(&self) -> Result<(Vec<Symbol>, TypeMap)> {
        get_symbol_info(&self.ads_client).await
    }

    /// Returns if the connected ADS device is in run mode.
    pub async fn is_run_mode(&self) -> Result<bool> {
        let state_info = self.ads_client.read_state().await?;
        Ok(state_info.ads_state == ads::AdsState::Run)
    }

    /// Attempts to set the ADS device into run mode if it is not already in it.
    pub async fn set_to_run_mode(&self) -> Result<()> {
        let device_info = self.ads_client.read_device_info().await?;
        println!("Device Info: {:?}", device_info);

        let state_info = self.ads_client.read_state().await?;
        println!("Device State: {:?}", state_info);

        if state_info.ads_state != ads::AdsState::Run {
            println!("Attempting to set PLC to run mode...");

            let next_state_info = StateInfo {
                ads_state: ads::AdsState::Run,
                device_state: state_info.device_state,
            };
            self.ads_client
                .write_control(&next_state_info, None)
                .await?;

            let state_info = self.ads_client.read_state().await?;
            println!("Device State: {:?}", state_info);
            assert_eq!(state_info.ads_state, ads::AdsState::Run);
        }

        Ok(())
    }

    /// Read the value of a symbol with a given type once.
    pub async fn read_symbol<T: AdsData>(&mut self, symbol: &str) -> Result<T> {
        let mut read_data = T::default();

        let index_offset = self.symbol_handle(symbol).await?.as_u32();

        read_exact(
            &self.ads_client,
            RW_SYMVAL_BYHANDLE,
            index_offset,
            read_data.as_bytes_mut(),
        )
        .await?;

        Ok(read_data)
    }

    /// Gets a type tree for the symbol to get the format of a symbol's internal structure at runtime.
    pub async fn get_dynamic_type_tree(&self, symbol: &str) -> Result<SymbolTypeTree> {
        let symbol_name = symbol;
        let (symbols, type_map) = get_symbol_info(&self.ads_client).await?;
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
                println!("Error when getting symbol type from type map {err:?}");
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
        let write_data = params.as_data();

        read_write_exact(
            &self.ads_client,
            RW_SYMVAL_BYHANDLE,
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
        let write_data = params.as_data();

        read_write_exact(
            &self.ads_client,
            RW_SYMVAL_BYHANDLE,
            index_offset,
            read_data.as_bytes_mut(),
            &write_data,
        )
        .await?;

        Ok(read_data)
    }

    /// Subscribes to a symbol and returns the notification handle.
    pub async fn subscribe<T: AdsData + Send + Sync + 'static>(
        &mut self,
        symbol: &str,
    ) -> Result<NotificationSubscription<T>> {
        let index_offset = self.symbol_handle(symbol).await?.as_u32();

        let attributes = AdsNotificationAttrib {
            cb_length: T::size() as u32,
            trans_mode: ads::AdsTransMode::OnChange,
            // max_delay in units of 100ns
            max_delay: 0,
            // cycle_time in units of 100ns
            // TODO: setting this to higher e.g: 1000ms does not work, maybe because the status data is changing every PLC cycle?
            // NB: Setting this to 10ms to match the PLC cycle time that it seems to be reporting at anyway
            cycle_time: 100_000, // (100,000 * 100ns) = 10,000,000 ns = 10 ms
        };
        let mut ads_handle: u32 = 0;

        let (notification_tx, notification_rx) = mpsc::unbounded_channel();
        let callback = move |_handle, _timestamp, payload| {
            // TODO(mw): Do we need to handle this failure better?
            let data = T::from_bytes(payload)
                .expect("Failed to parse PlcDataType from notification bytes");
            notification_tx.send(data);
        };

        self.ads_client
            .add_device_notification(
                RW_SYMVAL_BYHANDLE,
                index_offset,
                &attributes,
                &mut ads_handle,
                callback,
            )
            .await?;

        let handle = NotificationHandle::new(ads_handle.clone());

        self.notification_handles.push(handle.clone());

        let subscription = NotificationSubscription {
            receiver: notification_rx,
            handle: handle,
        };

        Ok(subscription)
    }

    /// Unsubscribe from a symbol notification subscription.
    pub async fn unsubscribe<T>(&self, subscription: NotificationSubscription<T>) -> Result<()> {
        self.unsubscribe_handle(&subscription.handle).await
    }

    /// Deletes all ongoing symbol notification subscriptions.
    pub async fn unsubscribe_all(&mut self) -> Result<()> {
        for notification_handle in &self.notification_handles {
            self.unsubscribe_handle(notification_handle).await;
        }

        self.notification_handles.clear();

        Ok(())
    }

    async fn unsubscribe_handle(&self, handle: &NotificationHandle) -> Result<()> {
        handle.cancel();

        self.ads_client
            .delete_device_notification(handle.ads_handle)
            .await?;

        Ok(())
    }
}

pub(crate) async fn read_exact(
    ads_client: &ads::Client,
    index_group: u32,
    index_offset: u32,
    data: &mut [u8],
) -> Result<()> {
    let len = ads_client.read(index_group, index_offset, data).await?;
    if len != data.len() as u32 {
        Err(AdsError::Reply(
            "read data",
            "got less data than expected",
            len as u32,
        ))
    } else {
        Ok(())
    }
}

pub(crate) async fn read_write_exact(
    ads_client: &ads::Client,
    index_group: u32,
    index_offset: u32,
    read_data: &mut [u8],
    write_data: &[u8],
) -> Result<()> {
    let len = ads_client
        .read_write(index_group, index_offset, read_data, write_data)
        .await?;
    if len != read_data.len() as u32 {
        Err(AdsError::Reply(
            "read/write data",
            "got less data than expected",
            len as u32,
        ))
    } else {
        Ok(())
    }
}
