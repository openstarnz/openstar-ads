use ads_client as ads;
use byteorder::LE;
use std::{collections::HashMap, time::Duration};

pub use ads::AdsTimeout;

use crate::{PlcConnection, PlcDataType, PlcError, PlcParams, SymbolTypeTree, SymbolTypeTreeError};

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
    symbol_handles: HashMap<String, SymbolHandle>,
    notification_handles: Vec<NotificationHandle>,
}

/// Get u32 handle to the name in the write data. Index offset is 0.
const GET_SYMHANDLE_BYNAME: u32 = 0xF003;
const GET_SYMHANDLE_BYNAME_LEN: usize = 4;

/// Provides a more user friendly wrapper to interact with the OpenStar PLC's.
impl PlcClient {
    pub fn new(ads_client: ads::Client) -> Self {
        Self {
            ads_client,
            symbol_handles: Default::default(),
            notification_handles: Default::default(),
        }
    }

    async fn print_device_info(&self) {
        let device_info = self.ads_client.read_device_info().await?;

        println!(
            "DeviceInfo: TwinCAT {}.{}.{} , Device Name: {}",
            device_info.major, device_info.minor, device_info.build, device_info.device_name
        );
    }

    async fn print_state(&self) {
        let state = self.ads_client.read_state().await?;

        println!("State: {:?}", state);
    }

    async fn get_symbol_handle(&self, symbol: &str) -> Result<SymbolHandle, PlcError> {
        let mut read_data = [0; GET_SYMHANDLE_BYNAME_LEN];
        let write_data = symbol.as_bytes();

        self.ads_client
            .read_write(GET_SYMHANDLE_BYNAME, 0, &mut read_data, write_data)
            .await?;

        Ok(SymbolHandle(u32::from_le_bytes(read_data)))
    }

    async fn symbol_handle(&mut self, symbol: &str) -> Result<SymbolHandle, PlcError> {
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

    async fn get_symbol_info(&self, symbol: &str) -> Result<SymbolHandle, PlcError> {
        let mut read_data = [0; GET_SYMHANDLE_BYNAME_LEN];

        if let Err(err) = self.ads_client.read(0xF00F, 0, &mut read_data).await {
            println!("Error: {}", err);
        }

        let n_symbols = LE::read_u32(&read_data[0..]) as usize;
        let symbol_len = LE::read_u32(&read_data[4..]) as usize;
        let n_types = LE::read_u32(&read_data[8..]) as usize;
        let types_len = LE::read_u32(&read_data[12..]) as usize;

        let mut symbol_data: Vec<u8> = Vec::with_capacity(symbol_len);
        symbol_data.resize(symbol_len, 0);

        let mut types_data: Vec<u8> = Vec::with_capacity(types_len);
        types_data.resize(types_len, 0);
    }

    /// Returns if the connected ADS device is in run mode.
    pub async fn is_run_mode(&self) -> Result<bool, PlcError> {
        let state_info = self.ads_client.read_state().await?;
        Ok(state_info.ads_state == ads::AdsState::Run)
    }

    /// Attempts to set the ADS device into run mode if it is not already in it.
    pub fn set_to_run_mode(&self) -> Result<(), PlcError> {
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
    pub fn read_symbol<T: PlcDataType>(&self, symbol: &str) -> Result<T, PlcError> {
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
    ) -> Result<(), PlcError> {
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
    ) -> Result<Value, PlcError> {
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
    ) -> Result<NotificationHandle, PlcError> {
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
    pub fn get_dynamic_type_tree(&self, name: &str) -> Result<SymbolTypeTree, PlcError> {
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
    ) -> Result<u32, PlcError> {
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
