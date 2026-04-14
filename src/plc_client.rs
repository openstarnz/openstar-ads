use crossbeam_channel::Receiver;
use std::collections::HashMap;

use crate::{
    data_types::{
        symbol_type_tree::{SymbolTypeTree, SymbolTypeTreeError},
        PlcDataType,
    },
    PlcParams,
};

#[derive(Debug, thiserror::Error)]
pub enum PlcClientError {
    #[error("PLC Client had ADS error {0}")]
    Ads(#[from] ads::Error),

    #[error("Symbol Type Tree error {0}")]
    SymbolTypeTree(#[from] SymbolTypeTreeError),
}

#[derive(Debug, Copy, Clone)]
pub struct SymbolHandle(u32);

impl SymbolHandle {
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Copy, Clone)]
pub struct NotificationHandle(u32);

impl NotificationHandle {
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

pub struct PlcClient {
    ads_client: ads::Client,
    plc_ams_address: ads::AmsAddr,
    symbol_handles: HashMap<String, SymbolHandle>,
    notification_handles: Vec<NotificationHandle>,
}

/// Provides a more user friendly wrapper to the Client, Device, and Handle objects from ads. Handles all of the necessary calls to
/// the underlying types and internal offsets required to talk to an ADS device.
impl PlcClient {
    pub fn new(ads_client: ads::Client, plc_ams_address: ads::AmsAddr) -> Self {
        Self {
            ads_client,
            plc_ams_address,
            symbol_handles: Default::default(),
            notification_handles: Default::default(),
        }
    }

    fn ads_client(&self) -> &ads::Client {
        &self.ads_client
    }

    fn ads_device<'a>(&'a self) -> ads::Device<'a> {
        self.ads_client().device(self.plc_ams_address)
    }

    fn symbol_handle(&mut self, symbol: &str) -> Result<SymbolHandle, PlcClientError> {
        // TODO: might need to think a bit more about other cases we may need to invalidate these handles e.g: new code flashed onto the PLC
        let handle = self.symbol_handles.get(symbol);
        let handle = match handle {
            Some(handle) => handle.to_owned(),
            None => {
                let handle_raw = ads::Handle::new(self.ads_device(), symbol)?.raw();
                let handle = SymbolHandle(handle_raw);
                self.symbol_handles.insert(symbol.to_owned(), handle);
                handle
            }
        };

        Ok(handle)
    }

    /// Returns if the connected ADS device is in run mode.
    pub fn is_run_mode(&self) -> Result<bool, PlcClientError> {
        let (state, _) = self.ads_device().get_state()?;

        Ok(state == ads::AdsState::Run)
    }

    /// Attempts to set the ADS device into run mode if it is not already in it.
    pub fn set_to_run_mode(&self) -> Result<(), PlcClientError> {
        let device = self.ads_device();

        println!("Device Info: {:?}", device.get_info());

        let (state, dev_state) = device.get_state()?;

        println!("Device State: {:?}", (state, dev_state));

        if state != ads::AdsState::Run {
            println!("Attempting to set PLC to run mode...");

            device.write_control(ads::AdsState::Run, dev_state)?;

            println!("Device State: {:?}", device.get_state());
        }

        assert!(device.get_state()?.0 == ads::AdsState::Run);

        Ok(())
    }

    /// Read the value of a symbol with a given type once.
    pub fn read_symbol<T: PlcDataType>(&self, symbol: &str) -> Result<T, PlcClientError> {
        let mut read_data = T::default();

        let index_offset = self.symbol_handle(symbol)?.as_u32();

        self.ads_device().read_exact(
            ads::index::RW_SYMVAL_BYHANDLE,
            index_offset,
            read_data.as_bytes_mut(),
        )?;

        Ok(read_data)
    }

    /// Invokes a PLC method (which has the attribute 'TcRpcEnable') with parameters.
    pub fn invoke_rpc_method<Params: PlcParams>(
        &mut self,
        symbol: &str,
        params: Params,
    ) -> Result<(), PlcClientError> {
        let index_offset = self.symbol_handle(symbol)?.as_u32();
        let write_data = params.as_data();

        self.ads_device().write_read_exact(
            ads::index::RW_SYMVAL_BYHANDLE,
            index_offset,
            &write_data,
            &mut [],
        )?;

        Ok(())
    }

    /// Invokes a PLC method (which has the attribute 'TcRpcEnable') and returns the resulting value.
    pub fn fetch_from_rpc_method<Params: PlcParams, Value: PlcDataType>(
        &mut self,
        symbol: &str,
        params: Params,
    ) -> Result<Value, PlcClientError> {
        let index_offset = self.symbol_handle(symbol)?.as_u32();
        let write_data = params.as_data();
        let mut read_data = Value::default();

        self.ads_device().write_read_exact(
            ads::index::RW_SYMVAL_BYHANDLE,
            index_offset,
            &write_data,
            read_data.as_bytes_mut(),
        )?;

        Ok(read_data)
    }

    /// Subscribes to a symbol and returns the notification handle.
    pub fn subscribe<T: PlcDataType>(
        &mut self,
        name: &str,
    ) -> Result<NotificationHandle, PlcClientError> {
        let notification_handle_raw = {
            let index_offset = self.symbol_handle(name)?.as_u32();

            self.ads_device().add_notification(
                ads::index::RW_SYMVAL_BYHANDLE,
                index_offset,
                &ads::notif::Attributes::new(
                    T::size(),
                    ads::notif::TransmissionMode::ServerOnChange,
                    std::time::Duration::ZERO,
                    // TODO: setting this to higher e.g: 1000ms does not work, maybe because the status data is changing every PLC cycle?
                    // NB: Setting this to 10ms to match the PLC cycle time that it seems to be reporting at anyway
                    std::time::Duration::from_millis(10),
                ),
            )?
        };
        let notification_handle = NotificationHandle(notification_handle_raw);
        self.notification_handles.push(notification_handle);

        Ok(notification_handle)
    }

    /// Gets a type tree for the symbol to get the format of a symbol's internal structure at runtime.
    pub fn get_dynamic_type_tree(&self, name: &str) -> Result<SymbolTypeTree, PlcClientError> {
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
    pub fn add_dynamic_symbol_notification(
        &mut self,
        symbol: &str,
        symbol_type_tree: &SymbolTypeTree,
    ) -> Result<u32, PlcClientError> {
        let index_offset = self.symbol_handle(symbol)?.as_u32();

        let notif_handle = self.ads_device().add_notification(
            ads::index::RW_SYMVAL_BYHANDLE,
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
    pub fn notification_receiver(&self) -> Receiver<ads::notif::Notification> {
        self.ads_client().get_notification_channel()
    }

    /// Deletes an ongoing subscription using its handle.
    pub fn unsubscribe(&self, notification_handle: u32) {
        // NB: unsure why, but deleting the notification returns a "Notification handle is invalid" error, hence the ok() here.
        // It still works though, so maybe not a problem.
        self.device().delete_notification(notification_handle).ok();
    }

    /// Deletes all ongoing symbol subscriptions.
    pub fn unsubscribe_all(&mut self) {
        for notification_handle in &self.notification_handles {
            self.unsubscribe(*notification_handle);
        }

        self.notification_handles.clear();
    }
}
