use anyhow::Result;
use esp_idf_svc::nvs::{EspNvs, NvsDefault};
use log;

const NVS_NAMESPACE: &str = "kurokku";

/// Application configuration.
/// Values are loaded from NVS first, falling back to compile-time env vars.
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
}

impl AppConfig {
    /// Load config from NVS, falling back to compile-time defaults.
    pub fn load(nvs: &EspNvs<NvsDefault>) -> Self {
        let wifi_ssid = nvs_get_str(nvs, "wifi_ssid")
            .unwrap_or_else(|| compile_default("KUROKKU_WIFI_SSID", ""));

        let wifi_password = nvs_get_str(nvs, "wifi_pass")
            .unwrap_or_else(|| compile_default("KUROKKU_WIFI_PASSWORD", ""));

        let server_url = nvs_get_str(nvs, "server_url")
            .unwrap_or_else(|| compile_default("KUROKKU_SERVER_URL", "http://192.168.1.100:8080"));

        let device_id = nvs_get_str(nvs, "device_id")
            .unwrap_or_else(|| compile_default("KUROKKU_DEVICE_ID", "esp32-001"));

        let format_24h = nvs_get_u8(nvs, "format_24h").map(|v| v != 0).unwrap_or(true);
        let default_brightness = nvs_get_u8(nvs, "brightness").unwrap_or(4);
        let poll_interval_ms = nvs_get_u32(nvs, "poll_ms").map(|v| v as u64).unwrap_or(5000);

        let tz = nvs_get_str(nvs, "tz")
            .unwrap_or_else(|| compile_default("KUROKKU_TZ", "CST6CDT,M3.2.0,M11.1.0"));

        let cfg = Self {
            wifi_ssid,
            wifi_password,
            server_url,
            device_id,
            format_24h,
            default_brightness,
            poll_interval_ms,
            tz,
        };

        log::info!(
            "Config loaded: server={}, device={}, 24h={}, brightness={}, poll={}ms, tz={}",
            cfg.server_url,
            cfg.device_id,
            cfg.format_24h,
            cfg.default_brightness,
            cfg.poll_interval_ms,
            cfg.tz,
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
    // First get the length
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
    // NVS doesn't have u32 directly in all esp-idf-svc versions;
    // store as i32 and cast
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

/// Get a compile-time env var, falling back to a default.
fn compile_default(env_key: &str, default: &str) -> String {
    option_env_dynamic(env_key).unwrap_or_else(|| default.to_string())
}

/// Workaround: option_env! is a macro that needs a literal.
/// We use known keys and match.
fn option_env_dynamic(key: &str) -> Option<String> {
    match key {
        "KUROKKU_WIFI_SSID" => option_env!("KUROKKU_WIFI_SSID").map(|s| s.to_string()),
        "KUROKKU_WIFI_PASSWORD" => option_env!("KUROKKU_WIFI_PASSWORD").map(|s| s.to_string()),
        "KUROKKU_SERVER_URL" => option_env!("KUROKKU_SERVER_URL").map(|s| s.to_string()),
        "KUROKKU_DEVICE_ID" => option_env!("KUROKKU_DEVICE_ID").map(|s| s.to_string()),
        "KUROKKU_TZ" => option_env!("KUROKKU_TZ").map(|s| s.to_string()),
        _ => None,
    }
}
