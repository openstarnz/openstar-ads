use ads_client::{self as ads, AdsNotificationAttrib, StateInfo};
use std::{collections::HashMap, fs::read};
use tokio::sync::watch;

use crate::{get_symbol_info, PlcDataType, PlcError, PlcParams, Result, Symbol, TypeMap};

#[derive(Debug, Copy, Clone)]
pub struct SymbolHandle(u32);

impl SymbolHandle {
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

#[derive(Debug)]
pub struct NotificationSubscription<T> {
    receiver: watch::Receiver<T>,
    handle: u32,
}

pub struct AdsClient {
    ads_client: ads::Client,
    symbol_handles: HashMap<String, SymbolHandle>,
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
    pub async fn read_symbol<T: PlcDataType>(&mut self, symbol: &str) -> Result<T> {
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

    /// Invokes a PLC method (which has the attribute 'TcRpcEnable') with parameters.
    pub async fn invoke_rpc_method<Params: PlcParams>(
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
    pub async fn fetch_from_rpc_method<Params: PlcParams, Value: PlcDataType>(
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
    /// TODO
    pub async fn subscribe<T: PlcDataType>(
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
        let handle: u32 = 0;

        let (notification_tx, notification_rx) = watch::channel(T::default());
        let callback = |handle, timestamp, payload| {
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
                &mut handle,
                callback,
            )
            .await?;

        let subscription = NotificationSubscription {
            receiver: notification_rx,
            handle,
        };

        Ok(subscription)
    }

    pub async fn unsubscribe<T>(&self, subscription: NotificationSubcription<T>) -> Result<()> {
        self.ads_client
            .delete_device_notification(subscription.handle)
            .await
            .map_err(Into::into)
    }

    /*

            /// Deletes an ongoing subscription using its handle.
    /// TODO
    pub fn unsubscribe(&self, notification_handle: u32) {
        // NB: unsure why, but deleting the notification returns a "Notification handle is invalid" error, hence the ok() here.
        // It still works though, so maybe not a problem.
        self.device().delete_notification(notification_handle).ok();
    }
     */

    /// Gets a type tree for the symbol to get the format of a symbol's internal structure at runtime.
    /// TODO
    pub fn get_dynamic_type_tree(&self, name: &str) -> Result<SymbolTypeTree> {
        let (symbols, type_map) = ads::symbol::get_symbol_info(self.ads_device())?;
        let mut symbol = None;

        for symbol_info in symbols {
            if symbol_info.name.to_lowercase() == name.to_lowercase() {
                symbol = Some(symbol_info);
            }
        }

        let Some(symbol) = symbol else {
            return Ok(SymbolTypeTree::Missing);
        };

        let symbol_type_tree = match (&symbol, &type_map).try_into() {
            Ok(symbol_type_tree) => symbol_type_tree,
            Err(err) => {
                println!("Error when getting symbol type from type map {err:?}");
                return Ok(SymbolTypeTree::Unknown(symbol.size));
            }
        };

        Ok(symbol_type_tree)
    }

    /// Subscribes to a symbol based off of its type tree and returns the handle as a u32.
    /// TODO
    pub fn add_dynamic_symbol_notification(
        &mut self,
        symbol: &str,
        symbol_type_tree: &SymbolTypeTree,
    ) -> Result<u32> {
        let index_offset = self.symbol_handle(symbol)?.as_u32();

        let notif_handle = self.ads_device().add_notification(
            RW_SYMVAL_BYHANDLE,
            index_offset,
            &ads::notif::Attributes::new(
                symbol_type_tree.get_size(),
                ads::notif::TransmissionMode::ServerOnChange,
                std::time::Duration::ZERO,
                std::time::Duration::from_millis(10),
            ),
        )?;

        Ok(notif_handle)
    }

    /// Get a crossbeam channel receiver for all ADS notifications.
    /// TODO
    pub fn notification_receiver(&self) -> Receiver<ads::notif::Notification> {
        self.ads_client().get_notification_channel()
    }

    /// Deletes all ongoing symbol subscriptions.
    /// TODO
    pub fn unsubscribe_all(&mut self) {
        for notification_handle in &self.notification_handles {
            self.unsubscribe(*notification_handle);
        }

        self.notification_handles.clear();
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
        Err(PlcError::Reply(
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
        Err(PlcError::Reply(
            "read/write data",
            "got less data than expected",
            len as u32,
        ))
    } else {
        Ok(())
    }
}
