use std::time::Duration;

use sysinfo::{InterfaceOperationalState, NetworkData};

#[derive(Clone, Debug)]
pub struct NetworkItem {
    pub name: String,
    pub state: InterfaceOperationalState,
    pub rx: f64,
    pub tx: f64,
}

impl NetworkItem {
    pub fn new(name: &str, data: &NetworkData, refresh: Duration) -> Self {
        Self {
            name: name.into(),
            state: data.operational_state(),
            rx: (data.received() as f64) / refresh.as_secs_f64(),
            tx: (data.transmitted() as f64) / refresh.as_secs_f64(),
        }
    }
}
