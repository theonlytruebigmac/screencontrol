//! System information collector.

use serde::Serialize;
use sysinfo::{Disks, Networks, System};

#[derive(Debug, Clone, Serialize)]
pub struct SystemInfo {
    pub machine_name: String,
    pub os: String,
    pub os_version: String,
    pub arch: String,
    pub cpu_count: usize,
    pub total_memory: u64,
}

/// Real-time metrics sampled each heartbeat tick.
pub struct RealtimeMetrics {
    pub cpu_usage: f64,
    pub memory_used: u64,
    pub memory_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub uptime_secs: u32,
    pub ip_address: String,
}

/// Collect current system information.
pub fn collect_system_info() -> SystemInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    SystemInfo {
        machine_name: System::host_name().unwrap_or_else(|| "unknown".into()),
        os: std::env::consts::OS.to_string(),
        os_version: System::os_version().unwrap_or_else(|| "unknown".into()),
        arch: std::env::consts::ARCH.to_string(),
        cpu_count: sys.cpus().len(),
        total_memory: sys.total_memory(),
    }
}

/// Sample real-time metrics from an existing `System` handle.
/// Caller should call `sys.refresh_cpu_all()` + `sys.refresh_memory()` before
/// calling this so the values are fresh.
pub fn collect_realtime_metrics(sys: &mut System) -> RealtimeMetrics {
    sys.refresh_cpu_all();
    sys.refresh_memory();

    let cpu_usage = sys.global_cpu_usage() as f64;
    let memory_used = sys.used_memory();
    let memory_total = sys.total_memory();

    // Aggregate disk usage
    let disks = Disks::new_with_refreshed_list();
    let (mut disk_used, mut disk_total) = (0u64, 0u64);
    for disk in disks.list() {
        disk_total += disk.total_space();
        disk_used += disk.total_space() - disk.available_space();
    }

    let uptime_secs = System::uptime() as u32;

    // Get primary IP address (first non-loopback IPv4)
    let ip_address = get_primary_ip();

    RealtimeMetrics {
        cpu_usage,
        memory_used,
        memory_total,
        disk_used,
        disk_total,
        uptime_secs,
        ip_address,
    }
}

/// Get the primary non-loopback IPv4 address.
fn get_primary_ip() -> String {
    let networks = Networks::new_with_refreshed_list();
    for (name, data) in networks.iter() {
        // Skip loopback
        if name == "lo"
            || name.starts_with("docker")
            || name.starts_with("veth")
            || name.starts_with("br-")
        {
            continue;
        }
        for ip in data.ip_networks() {
            if ip.addr.is_ipv4() && !ip.addr.is_loopback() {
                return ip.addr.to_string();
            }
        }
    }
    String::new()
}
