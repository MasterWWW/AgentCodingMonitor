//! mDNS registration for watch companion discovery (`_vibe-monitor._tcp`).

use crate::lan::local_ipv4_addresses;
use crate::state::{ensure_device_id, load_watch_companion_enabled, watch_service_name};
use anyhow::{Context, Result};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::net::Ipv4Addr;
use tracing::{info, warn};

const SERVICE_TYPE: &str = "_vibe-monitor._tcp.local.";
const MDNS_VERSION: &str = "1";

pub struct MdnsRegistrar {
    _daemon: ServiceDaemon,
}

impl MdnsRegistrar {
    /// 启用手表伴侣时注册 mDNS；未启用时返回 `None`。
    pub fn register(port: u16) -> Result<Option<Self>> {
        if !load_watch_companion_enabled() {
            return Ok(None);
        }
        let device_id = ensure_device_id()?;
        let instance = watch_service_name(&device_id);
        let ips = local_ipv4_addresses();
        if ips.is_empty() {
            warn!("watch companion mDNS: no LAN IPv4 addresses found");
            return Ok(None);
        }

        let daemon = ServiceDaemon::new().context("mdns ServiceDaemon")?;
        let host = format!("vibe-monitor-{device_id}.local.");
        let props = vec![
            ("device_id".to_string(), device_id.clone()),
            ("version".to_string(), MDNS_VERSION.to_string()),
        ];
        let info = build_service_info(&instance, &host, &ips, port, &props)?;
        daemon.register(info).context("mdns register")?;
        info!(
            "watch companion mDNS registered: {} port {} ips {:?}",
            instance, port, ips
        );
        Ok(Some(Self { _daemon: daemon }))
    }
}

fn build_service_info(
    instance: &str,
    host: &str,
    ips: &[Ipv4Addr],
    port: u16,
    props: &[(String, String)],
) -> Result<ServiceInfo> {
    let ip = ips.first().context("no LAN IPv4 for mDNS")?;
    ServiceInfo::new(
        SERVICE_TYPE,
        instance,
        host,
        &ip.to_string(),
        port,
        props,
    )
    .context("ServiceInfo::new")
}
