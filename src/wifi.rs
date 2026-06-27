use anyhow::Result;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
pub use esp_idf_svc::sntp::EspSntp;
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

/// Read the current AP signal strength (RSSI) in dBm, or None if the station
/// isn't associated. Backed by the global `esp_wifi_sta_get_rssi`, so it does
/// not need the WiFi handle locked — the parameter just documents that a
/// connection is expected.
pub fn get_rssi(_wifi: &BlockingWifi<EspWifi<'static>>) -> Option<i32> {
    let mut rssi: core::ffi::c_int = 0;
    let err = unsafe { esp_idf_svc::sys::esp_wifi_sta_get_rssi(&mut rssi) };
    if err == esp_idf_svc::sys::ESP_OK {
        Some(rssi as i32)
    } else {
        None
    }
}

/// Get the station IP address as a string, or None if not connected.
pub fn get_ip(wifi: &BlockingWifi<EspWifi<'static>>) -> Option<String> {
    wifi.wifi()
        .sta_netif()
        .get_ip_info()
        .ok()
        .map(|info| format!("{}", info.ip))
}

/// Start SNTP and block until the first sync completes or a 15s timeout.
///
/// Returns the SNTP handle, which the caller **must keep alive for the lifetime
/// of the program**: dropping it calls `sntp_stop()` and kills periodic re-sync
/// (ESP-IDF re-syncs every hour by default via `CONFIG_LWIP_SNTP_UPDATE_DELAY`).
///
/// A timeout is *not* fatal — the returned handle keeps polling in the
/// background, so a device that boots before its NTP server is reachable will
/// still sync once the network recovers, instead of staying on a wrong time
/// until reboot.
///
/// `ntp_server`, when set, overrides the primary pool server (NVS `ntp_server`).
pub fn sync_ntp(ntp_server: Option<&str>) -> Result<EspSntp<'static>> {
    use esp_idf_svc::sntp::{EspSntp, SntpConf};

    let mut conf = SntpConf::default();
    match ntp_server.map(str::trim).filter(|s| !s.is_empty()) {
        Some(server) => {
            // Override the primary server; any remaining slots keep their
            // pool.ntp.org defaults as fallback (lwIP tries them in order).
            conf.servers[0] = server;
            log::info!("Starting NTP sync (server override: {})...", server);
        }
        None => log::info!("Starting NTP sync..."),
    }

    // The callback fires on every completed sync, including the hourly
    // background re-syncs that ESP-IDF performs on its own. `synced` is the
    // wall-clock time as a Duration since the epoch.
    let sntp = EspSntp::new_with_callback(&conf, |synced| {
        log::info!("NTP time synced (epoch: {}s)", synced.as_secs());
    })
    .map_err(|e| anyhow::anyhow!("SNTP init failed: {}", e))?;

    // Block until the first sync lands so the engine starts with correct time.
    // The callback above already logs the sync itself, so don't log again here.
    let start = std::time::Instant::now();
    while sntp.get_sync_status() != esp_idf_svc::sntp::SyncStatus::Completed {
        if start.elapsed() > Duration::from_secs(15) {
            // Hand the still-running handle back so background re-sync continues.
            log::warn!("NTP first sync not completed within 15s; continuing in background");
            return Ok(sntp);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(sntp)
}

/// If the STA is disconnected, try to reconnect once. Returns Ok(true) if a
/// reconnect was attempted and succeeded, Ok(false) if already connected.
/// On reconnect failure, returns Err — caller should keep running and retry
/// on the next poll interval rather than panicking.
pub fn reconnect_if_down(wifi: &mut BlockingWifi<EspWifi<'static>>) -> Result<bool> {
    let connected = wifi
        .is_connected()
        .map_err(|e| anyhow::anyhow!("is_connected failed: {}", e))?;
    if connected {
        return Ok(false);
    }

    log::warn!("WiFi disconnected — attempting reconnect");
    wifi.connect()
        .map_err(|e| anyhow::anyhow!("reconnect failed: {}", e))?;
    wifi.wait_netif_up()
        .map_err(|e| anyhow::anyhow!("netif up failed after reconnect: {}", e))?;
    log::info!("WiFi reconnected");
    Ok(true)
}

/// Apply a POSIX TZ string to the C runtime so `localtime_r` honors it.
/// Call this after NTP sync.
pub fn set_timezone(tz: &str) -> Result<()> {
    use std::ffi::CString;
    let key = CString::new("TZ").unwrap();
    let val = CString::new(tz).map_err(|_| anyhow::anyhow!("invalid TZ string"))?;
    unsafe {
        esp_idf_svc::sys::setenv(key.as_ptr(), val.as_ptr(), 1);
        esp_idf_svc::sys::tzset();
    }
    log::info!("Timezone set: {}", tz);
    Ok(())
}
