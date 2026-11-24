use domain::provider::{PortHolder, app_name};

pub trait Ports: Send + Sync {
    fn holder(&self, port: u16) -> Option<PortHolder>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemPorts;

impl Ports for SystemPorts {
    fn holder(&self, port: u16) -> Option<PortHolder> {
        let (pid, executable) = platform::port_holder(port)?;
        Some(PortHolder { pid, app: app_name(&executable) })
    }
}
