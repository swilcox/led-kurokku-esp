//! Thin wrapper over the ESP-IDF internal temperature sensor.
//!
//! `esp-idf-svc` doesn't wrap the `temperature_sensor` driver, so we call the
//! raw `sys` bindings directly. The ESP32-C3 (and C6) have an on-die sensor
//! that reports a coarse junction temperature — useful for spotting a device
//! cooking in an enclosure, not for precision measurement.

use anyhow::Result;
use esp_idf_svc::sys;

/// An installed, enabled temperature sensor. Owns the driver handle; the raw
/// pointer makes it `!Send`, so create and read it from a single thread.
pub struct TempSensor {
    handle: sys::temperature_sensor_handle_t,
}

impl TempSensor {
    /// Install and enable the sensor. The measurement range is a hint that lets
    /// the driver pick an offset; readings outside it just lose accuracy.
    pub fn new() -> Result<Self> {
        // Start from a zeroed config so we stay forward-compatible with extra
        // fields (e.g. `flags`) added in later IDF versions, and only set the
        // range. clk_src defaults to TEMPERATURE_SENSOR_CLK_SRC_DEFAULT (0).
        let mut config = sys::temperature_sensor_config_t::default();
        config.range_min = -10;
        config.range_max = 80;

        let mut handle: sys::temperature_sensor_handle_t = core::ptr::null_mut();
        let err = unsafe { sys::temperature_sensor_install(&config, &mut handle) };
        if err != sys::ESP_OK {
            anyhow::bail!("temperature_sensor_install failed: {}", err);
        }
        let err = unsafe { sys::temperature_sensor_enable(handle) };
        if err != sys::ESP_OK {
            anyhow::bail!("temperature_sensor_enable failed: {}", err);
        }
        Ok(Self { handle })
    }

    /// Read the current die temperature in degrees Celsius, or None on error.
    pub fn read_celsius(&self) -> Option<f32> {
        let mut out: f32 = 0.0;
        let err = unsafe { sys::temperature_sensor_get_celsius(self.handle, &mut out) };
        if err == sys::ESP_OK {
            Some(out)
        } else {
            None
        }
    }
}
