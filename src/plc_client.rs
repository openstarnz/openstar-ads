use std::collections::{hash_map::Entry, HashMap};

use ads::{symbol::get_symbol_info, AmsAddr, Client, Device, Handle, Result};
use crossbeam_channel::Receiver;

use crate::data_types::{symbol_type_tree::SymbolTypeTree, PlcDataType};

pub struct PlcClient {
    safe_cell: PlcClientSelfCell,
    notification_handles: Vec<u32>,
}

// Using self_cell here so we can create a struct that owns an ads Client, Device, and set of Handles. It would not be possible
// otherwise as the Device and Handles must refer to the Client, and Rust does not make it easy to create self-referrential structs
// TODO: long term we should investigate the ads crate and find a better solution without self_cell
self_cell::self_cell!(
    struct PlcClientSelfCell {
        owner: Client,

        #[covariant]
        dependent: PlcDevice,
    }
);

struct PlcDevice<'c> {
    device: Device<'c>,
    handles: HashMap<String, Handle<'c>>,
}

impl<'c> PlcDevice<'c> {
    fn handle(&mut self, name: &str) -> Result<&Handle<'c>> {
        // TODO: might need to think a bit more about other cases we may need to invalidate these handles e.g: new code flashed onto the PLC
        let handle = match self.handles.entry(name.to_string()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(Handle::new(self.device, name)?),
        };

        Ok(handle)
    }
}

impl PlcClient {
    pub fn new(ads_client: Client, plc_ams_address: AmsAddr) -> Self {
        let safe_cell = PlcClientSelfCell::new(ads_client, |ads_client| PlcDevice {
            device: ads_client.device(plc_ams_address),
            handles: HashMap::default(),
        });

        Self {
            safe_cell,
            notification_handles: Default::default(),
        }
    }

    fn ads_client(&self) -> &Client {
        self.safe_cell.borrow_owner()
    }

    fn device(&self) -> Device {
        self.safe_cell.borrow_dependent().device
    }

    fn handle(&mut self, name: &str) -> Result<&Handle> {
        self.safe_cell
            .with_dependent_mut(|_, plc_device| plc_device.handle(name))
    }

    pub fn is_run_mode(&self) -> Result<bool> {
        let (state, _) = self.device().get_state()?;

        Ok(state == ads::AdsState::Run)
    }

    pub fn set_to_run_mode(&self) -> Result<()> {
        let device = self.device();

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

    pub fn read_symbol<T: PlcDataType>(&mut self, name: &str) -> Result<T> {
        let handle = self.handle(name)?;

        let mut read_data = T::default();

        let index_offset = handle.raw();

        self.device().read_exact(
            ads::index::RW_SYMVAL_BYHANDLE,
            index_offset,
            read_data.as_bytes_mut(),
        )?;

        Ok(read_data)
    }

    pub fn invoke_rpc_method(&mut self, name: &str) -> Result<()> {
        let handle = self.handle(name)?;

        let index_offset = handle.raw();

        self.device().write_read_exact(
            ads::index::RW_SYMVAL_BYHANDLE,
            index_offset,
            &[],
            &mut [],
        )?;

        Ok(())
    }

    pub fn invoke_rpc_method_with_param<P: PlcDataType>(
        &mut self,
        name: &str,
        param: P,
    ) -> Result<()> {
        let handle = self.handle(name)?;

        let write_data = param.as_bytes();

        let index_offset = handle.raw();

        self.device().write_read_exact(
            ads::index::RW_SYMVAL_BYHANDLE,
            index_offset,
            write_data,
            &mut [],
        )?;

        Ok(())
    }

    // TODO: there must be a more generic way to handle this
    pub fn invoke_rpc_method_with_three_params<
        P1: PlcDataType,
        P2: PlcDataType,
        P3: PlcDataType,
    >(
        &mut self,
        name: &str,
        param_1: P1,
        param_2: P2,
        param_3: P3,
    ) -> Result<()> {
        let handle = self.handle(name)?;

        let write_data_1 = param_1.as_bytes();
        let write_data_2 = param_2.as_bytes();
        let write_data_3 = param_3.as_bytes();
        let write_data = [write_data_1, write_data_2, write_data_3].concat();

        let index_offset = handle.raw();

        self.device().write_read_exact(
            ads::index::RW_SYMVAL_BYHANDLE,
            index_offset,
            &write_data,
            &mut [],
        )?;

        Ok(())
    }

    pub fn fetch_from_rpc_method<T: PlcDataType>(&mut self, name: &str) -> Result<T> {
        let handle = self.handle(name)?;

        let mut read_data = T::default();

        let index_offset = handle.raw();

        self.device().write_read_exact(
            ads::index::RW_SYMVAL_BYHANDLE,
            index_offset,
            &[],
            read_data.as_bytes_mut(),
        )?;

        Ok(read_data)
    }

    pub fn subscribe<T: PlcDataType>(&mut self, name: &str) -> Result<u32> {
        let notification_handle = {
            let handle = self.handle(name)?;

            let index_offset = handle.raw();

            self.device().add_notification(
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

        self.notification_handles.push(notification_handle);

        Ok(notification_handle)
    }

    pub fn get_dynamic_type_tree(&mut self, name: &str) -> Result<SymbolTypeTree> {
        let (symbols, type_map) = get_symbol_info(self.device())?;
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
            },
        };

        Ok(symbol_type_tree)
    }

    pub fn add_dynamic_symbol_notification(
        &mut self,
        name: &str,
        symbol_type_tree: &SymbolTypeTree,
    ) -> Result<u32> {
        let handle = self.handle(name)?;

        let index_offset = handle.raw();

        let notif_handle = self.device().add_notification(
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

    pub fn notification_receiver(&self) -> Receiver<ads::notif::Notification> {
        self.ads_client().get_notification_channel()
    }

    pub fn unsubscribe(&self, notification_handle: u32) {
        // NB: unsure why, but deleting the notification returns a "Notification handle is invalid" error, hence the ok() here.
        // It still works though, so maybe not a problem.
        self.device().delete_notification(notification_handle).ok();
    }

    pub fn unsubscribe_all(&mut self) {
        for notification_handle in &self.notification_handles {
            self.unsubscribe(*notification_handle);
        }

        self.notification_handles.clear();
    }
}
