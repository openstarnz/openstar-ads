use ads_client::AdsTimeout;
use std::time::Duration;

enum PlcConnection {
    Connected(ads_client::Client),
    Disconnected,
}

impl Default for PlcConnection {
    fn default() -> Self {
        Self::Disconnected
    }
}

impl PlcConnection {
    fn connect(
        &mut self,
        addr: String,
        port: u16,
        timeout: Option<AdsTimeout>,
        retry_delay: Option<Duration>,
        set_to_run_mode: bool,
    ) -> Result<(), PlcError> {
        match self {
            PlcConnection::Connected(_) => {
                println!("Attempted to connect to PLC but it is already connected!")
            }
            PlcConnection::Disconnected => {
                let ams_source: ads::Source =
                    local_ams_address.map_or(ads::Source::Request, ads::Source::Addr);

                let mut timeouts = ads::Timeouts::new(Duration::from_millis(1000));
                timeouts.read = Some(Duration::from_millis(2000));

                let ads_client = Client::new(ads_router_address, timeouts, ams_source)?;

                let plc_client = PlcClient::new(ads_client, plc_ams_address);

                if !plc_client.is_run_mode()? && set_to_run_mode {
                    plc_client.set_to_run_mode()?;
                }

                if !plc_client.is_run_mode()? {
                    return Err(anyhow!("PLC not in run mode, stopping connection."));
                }

                *self = PlcConnection::Connected(plc_client);
            }
        }

        Ok(())
    }

    fn disconnect(&mut self) {
        match self {
            PlcConnection::Connected(plc_client) => {
                plc_client.unsubscribe_all();

                *self = PlcConnection::Disconnected;

                println!("PLC connection was dropped.");
            }
            PlcConnection::Disconnected => {
                // Already disconnected...
            }
        }
    }

    fn client(&self) -> Option<&PlcClient> {
        match self {
            PlcConnection::Connected(plc_client) => Some(plc_client),
            PlcConnection::Disconnected => None,
        }
    }

    fn client_mut(&mut self) -> Option<&mut PlcClient> {
        match self {
            PlcConnection::Connected(plc_client) => Some(plc_client),
            PlcConnection::Disconnected => None,
        }
    }

    fn handle_disconnect_error(&mut self, error: &ads::Error) {
        let should_disconnect = matches!(
            error,
            ads::Error::Io(_, _)
                | ads::Error::Ads(_, _, 0x006)
                | ads::Error::Reply(_, "unexpected invoke ID", _)
        );

        if should_disconnect {
            println!("PLC client error indicates we should disconnect...");

            self.disconnect();
        }
    }
}
