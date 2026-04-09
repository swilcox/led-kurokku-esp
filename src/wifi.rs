use anyhow::Result;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use log;
use std::time::Duration;

/// Connect to WiFi as a station. Blocks until connected or fails.
pub fn connect(
    ssid: &str,
    password: &str,
    modem: esp_idf_svc::hal::modem::Modem,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> Result<BlockingWifi<EspWifi<'static>>> {
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, sysloop.clone(), Some(nvs))
            .map_err(|e| anyhow::anyhow!("WiFi driver init failed: {}", e))?,
        sysloop,
    )
    .map_err(|e| anyhow::anyhow!("BlockingWifi wrap failed: {}", e))?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: ssid.try_into().map_err(|_| anyhow::anyhow!("SSID too long"))?,
        password: password.try_into().map_err(|_| anyhow::anyhow!("Password too long"))?,
        ..Default::default()
    }))
    .map_err(|e| anyhow::anyhow!("WiFi config failed: {}", e))?;

    wifi.start().map_err(|e| anyhow::anyhow!("WiFi start failed: {}", e))?;
    log::info!("WiFi started, connecting...");

    wifi.connect().map_err(|e| anyhow::anyhow!("WiFi connect failed: {}", e))?;
    log::info!("WiFi connected, waiting for IP...");

    wifi.wait_netif_up().map_err(|e| anyhow::anyhow!("WiFi netif up failed: {}", e))?;

    let ip_info = wifi
        .wifi()
        .sta_netif()
        .get_ip_info()
        .map_err(|e| anyhow::anyhow!("Failed to get IP info: {}", e))?;
    log::info!("WiFi connected: IP={}", ip_info.ip);

    Ok(wifi)
}

/// Get the station IP address as a string, or None if not connected.
pub fn get_ip(wifi: &BlockingWifi<EspWifi<'static>>) -> Option<String> {
    wifi.wifi()
        .sta_netif()
        .get_ip_info()
        .ok()
        .map(|info| format!("{}", info.ip))
}

/// Sync system time from NTP. Blocks until time is set or timeout.
pub fn sync_ntp() -> Result<()> {
    use esp_idf_svc::sntp::EspSntp;

    log::info!("Starting NTP sync...");
    let sntp = EspSntp::new_default()
        .map_err(|e| anyhow::anyhow!("SNTP init failed: {}", e))?;

    // Wait up to 15 seconds for time sync
    let start = std::time::Instant::now();
    while sntp.get_sync_status() != esp_idf_svc::sntp::SyncStatus::Completed {
        if start.elapsed() > Duration::from_secs(15) {
            anyhow::bail!("NTP sync timed out after 15s");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    log::info!("NTP time synced");
    Ok(())
}
