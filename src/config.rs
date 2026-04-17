use anyhow::Result;
use esp_idf_svc::nvs::{EspNvs, NvsDefault};
use log;

const NVS_NAMESPACE: &str = "kurokku";

/// Application configuration, loaded exclusively from NVS.
///
/// Firmware binaries are generic: per-device values (WiFi credentials,
/// server URL, device ID, etc.) must be provisioned into NVS via
/// `tools/provision.py` before the device can do useful work. On an
/// unprovisioned device, `has_wifi_config()` returns false and the engine
/// falls back to a status widget showing "NO WIFI".
pub struct AppConfig {
    pub wifi_ssid: String,
    pub wifi_password: String,
    pub server_url: String,
    pub device_id: String,
    pub format_24h: bool,
    pub default_brightness: u8,
    pub poll_interval_ms: u64,
    /// POSIX TZ string, e.g. "CST6CDT,M3.2.0,M11.1.0" for US Central.
    pub tz: String,
    /// UDP syslog target as "host:port", e.g. "192.168.1.50:5514". None = disabled.
    pub syslog_host: Option<String>,
}

impl AppConfig {
    /// Load config from NVS. Missing keys fall back to placeholder defaults.
    pub fn load(nvs: &EspNvs<NvsDefault>) -> Self {
        let wifi_ssid = nvs_get_str(nvs, "wifi_ssid").unwrap_or_default();
        let wifi_password = nvs_get_str(nvs, "wifi_pass").unwrap_or_default();
        let server_url = nvs_get_str(nvs, "server_url").unwrap_or_default();
        let device_id =
            nvs_get_str(nvs, "device_id").unwrap_or_else(|| "unprovisioned".to_string());
        let format_24h = nvs_get_u8(nvs, "format_24h").map(|v| v != 0).unwrap_or(true);
        let default_brightness = nvs_get_u8(nvs, "brightness").unwrap_or(4);
        let poll_interval_ms = nvs_get_u32(nvs, "poll_ms").map(|v| v as u64).unwrap_or(5000);
        let tz = nvs_get_str(nvs, "tz").unwrap_or_else(|| "UTC0".to_string());
        let syslog_host = nvs_get_str(nvs, "syslog_host");

        let cfg = Self {
            wifi_ssid,
            wifi_password,
            server_url,
            device_id,
            format_24h,
            default_brightness,
            poll_interval_ms,
            tz,
            syslog_host,
        };

        log::info!(
            "Config loaded: server={}, device={}, 24h={}, brightness={}, poll={}ms, tz={}, syslog={:?}",
            cfg.server_url,
            cfg.device_id,
            cfg.format_24h,
            cfg.default_brightness,
            cfg.poll_interval_ms,
            cfg.tz,
            cfg.syslog_host,
        );

        cfg
    }

    /// Save current config to NVS.
    pub fn save(&self, nvs: &mut EspNvs<NvsDefault>) -> Result<()> {
        nvs_set_str(nvs, "wifi_ssid", &self.wifi_ssid)?;
        nvs_set_str(nvs, "wifi_pass", &self.wifi_password)?;
        nvs_set_str(nvs, "server_url", &self.server_url)?;
        nvs_set_str(nvs, "device_id", &self.device_id)?;
        nvs_set_u8(nvs, "format_24h", if self.format_24h { 1 } else { 0 })?;
        nvs_set_u8(nvs, "brightness", self.default_brightness)?;
        nvs_set_u32(nvs, "poll_ms", self.poll_interval_ms as u32)?;
        nvs_set_str(nvs, "tz", &self.tz)?;
        match &self.syslog_host {
            Some(h) => nvs_set_str(nvs, "syslog_host", h)?,
            None => {
                let _ = nvs.remove("syslog_host");
            }
        }
        log::info!("Config saved to NVS");
        Ok(())
    }

    /// Check if WiFi credentials are configured.
    pub fn has_wifi_config(&self) -> bool {
        !self.wifi_ssid.is_empty() && !self.wifi_password.is_empty()
    }
}

/// Open the kurokku NVS namespace.
pub fn open_nvs(
    partition: esp_idf_svc::nvs::EspDefaultNvsPartition,
) -> Result<EspNvs<NvsDefault>> {
    EspNvs::new(partition, NVS_NAMESPACE, true)
        .map_err(|e| anyhow::anyhow!("NVS open failed: {}", e))
}

// --- NVS helpers ---

fn nvs_get_str(nvs: &EspNvs<NvsDefault>, key: &str) -> Option<String> {
    let len = nvs.str_len(key).ok().flatten()?;
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u8; len];
    nvs.get_str(key, &mut buf)
        .ok()
        .flatten()
        .map(|s| s.trim_end_matches('\0').to_string())
        .filter(|s| !s.is_empty())
}

fn nvs_get_u8(nvs: &EspNvs<NvsDefault>, key: &str) -> Option<u8> {
    nvs.get_u8(key).ok().flatten()
}

fn nvs_get_u32(nvs: &EspNvs<NvsDefault>, key: &str) -> Option<u32> {
    // esp-idf-svc's NVS wrapper doesn't expose u32; store as i32 and cast.
    nvs.get_i32(key).ok().flatten().map(|v| v as u32)
}

fn nvs_set_str(nvs: &mut EspNvs<NvsDefault>, key: &str, value: &str) -> Result<()> {
    nvs.set_str(key, value)
        .map_err(|e| anyhow::anyhow!("NVS set_str({}) failed: {}", key, e))
}

fn nvs_set_u8(nvs: &mut EspNvs<NvsDefault>, key: &str, value: u8) -> Result<()> {
    nvs.set_u8(key, value)
        .map_err(|e| anyhow::anyhow!("NVS set_u8({}) failed: {}", key, e))
}

fn nvs_set_u32(nvs: &mut EspNvs<NvsDefault>, key: &str, value: u32) -> Result<()> {
    nvs.set_i32(key, value as i32)
        .map_err(|e| anyhow::anyhow!("NVS set_i32({}) failed: {}", key, e))
}
