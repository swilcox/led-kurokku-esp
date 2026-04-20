// Pure modules — build on every target so `cargo test --target <host>`
// can exercise them without an ESP toolchain.
mod display;
mod font;
mod font_7seg;
mod framebuf;

// Firmware modules — pull in `esp-idf-svc`, so they only exist on the ESP
// target. Host builds stop here; host `main()` is a stub.
#[cfg(target_os = "espidf")]
mod config;
#[cfg(target_os = "espidf")]
mod engine;
#[cfg(target_os = "espidf")]
mod network;
#[cfg(target_os = "espidf")]
mod ota;
#[cfg(target_os = "espidf")]
mod udp_log;
#[cfg(target_os = "espidf")]
mod widget;
#[cfg(target_os = "espidf")]
mod wifi;

#[cfg(not(target_os = "espidf"))]
fn main() {}

#[cfg(target_os = "espidf")]
use esp_idf_svc::eventloop::EspSystemEventLoop;
#[cfg(target_os = "espidf")]
use esp_idf_svc::hal::peripherals::Peripherals;
#[cfg(target_os = "espidf")]
use esp_idf_svc::nvs::EspDefaultNvsPartition;
#[cfg(target_os = "espidf")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "espidf")]
use std::time::Duration;

#[cfg(target_os = "espidf")]
fn main() {
    // Initialize ESP-IDF logging and system services
    esp_idf_svc::sys::link_patches();
    udp_log::init();

    log::info!("LED Kurokku ESP starting up...");

    let peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take().unwrap();
    let nvs_partition = EspDefaultNvsPartition::take().unwrap();

    // Load config from NVS. Provisioned via tools/provision.py.
    // Clone the partition handle — NVS partition is shared, WiFi needs one too
    let nvs = config::open_nvs(nvs_partition.clone()).expect("Failed to open NVS");
    let cfg = config::AppConfig::load(&nvs);
    drop(nvs); // release the NVS handle before WiFi needs the partition

    #[cfg(feature = "max7219")]
    {
        use display::{AnyDisplay, Display};

        let mut disp = display::max7219::Max7219::new(
            peripherals.spi2,
            peripherals.pins.gpio6,  // SCLK
            peripherals.pins.gpio7,  // MOSI
            peripherals.pins.gpio10, // CS
        )
        .expect("Failed to create MAX7219");

        disp.init().expect("Failed to init MAX7219");
        disp.set_brightness(cfg.default_brightness);
        log::info!("MAX7219 initialized");

        let any_disp = AnyDisplay::Pixel(Box::new(disp));
        run(cfg, any_disp, "max7219", peripherals.modem, sysloop, nvs_partition);
    }

    #[cfg(feature = "tm1637")]
    {
        use display::{AnyDisplay, Display};

        let mut disp = display::tm1637::Tm1637::new(
            peripherals.pins.gpio4, // CLK
            peripherals.pins.gpio5, // DIO
        )
        .expect("Failed to create TM1637");

        disp.init().expect("Failed to init TM1637");
        disp.set_brightness(cfg.default_brightness);
        log::info!("TM1637 initialized");

        let any_disp = AnyDisplay::Segment(Box::new(disp));
        run(cfg, any_disp, "tm1637", peripherals.modem, sysloop, nvs_partition);
    }

    #[cfg(not(any(feature = "max7219", feature = "tm1637")))]
    {
        log::error!(
            "No display feature enabled! Build with --features max7219 or --features tm1637"
        );
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

/// Shared post-display startup: show banner, connect WiFi, sync NTP, run engine.
/// Diverges after display initialization so both backends reuse this path.
#[cfg(all(target_os = "espidf", any(feature = "max7219", feature = "tm1637")))]
fn run(
    cfg: config::AppConfig,
    mut any_disp: display::AnyDisplay,
    display_type: &'static str,
    modem: esp_idf_svc::hal::modem::Modem,
    sysloop: EspSystemEventLoop,
    nvs_partition: EspDefaultNvsPartition,
) -> ! {
    use widget::{CancelToken, Widget};

    // Show startup banner. Widgets that don't yet support the active display
    // type silently return Err — that's fine here; we ignore it.
    {
        let banner = if matches!(any_disp, display::AnyDisplay::Pixel(_)) {
            "LED クロック"
        } else {
            "KUROKKU"
        };
        let cancel = CancelToken::with_timeout(Duration::from_secs(3));
        let mut status = widget::status::Status::new(banner, Duration::from_secs(3));
        let _ = status.run(&mut any_disp, &cancel);
    }

    if !cfg.has_wifi_config() {
        log::warn!("No WiFi credentials configured");
        let cancel = CancelToken::with_timeout(Duration::from_secs(3));
        let mut status = widget::status::Status::new("NO CFG", Duration::from_secs(3));
        let _ = status.run(&mut any_disp, &cancel);
    }

    let wifi_handle: Option<engine::SharedWifi> = if cfg.has_wifi_config() {
        match wifi::connect(&cfg.wifi_ssid, &cfg.wifi_password, modem, sysloop, nvs_partition) {
            Ok(w) => {
                let ip = wifi::get_ip(&w).unwrap_or_else(|| "?.?.?.?".to_string());
                log::info!("Connected: {}", ip);
                {
                    let cancel = CancelToken::with_timeout(Duration::from_secs(3));
                    let mut status = widget::status::Status::new(&ip, Duration::from_secs(3));
                    let _ = status.run(&mut any_disp, &cancel);
                }

                if let Some(ref host) = cfg.syslog_host {
                    udp_log::enable_udp(host, &cfg.device_id);
                }

                // Sync NTP — keep handle alive for periodic re-sync
                let _sntp = match wifi::sync_ntp() {
                    Ok(sntp) => Some(sntp),
                    Err(e) => {
                        log::warn!("NTP sync failed: {}", e);
                        None
                    }
                };

                // Apply timezone after time sync
                if let Err(e) = wifi::set_timezone(&cfg.tz) {
                    log::warn!("Timezone set failed: {}", e);
                }

                // Log OTA partition info
                match ota::running_partition_info() {
                    Ok(info) => log::info!("Firmware: {}", info),
                    Err(e) => log::warn!("Could not read OTA info: {}", e),
                }

                Some(Arc::new(Mutex::new(w)))
            }
            Err(e) => {
                log::warn!("WiFi failed: {}", e);
                let cancel = CancelToken::with_timeout(Duration::from_secs(3));
                let mut status = widget::status::Status::new("NO WIFI", Duration::from_secs(3));
                let _ = status.run(&mut any_disp, &cancel);
                None
            }
        }
    } else {
        None
    };

    let source: Box<dyn network::InstructionSource> = Box::new(
        network::polling::HttpPoller::new(
            &cfg.server_url,
            &cfg.device_id,
            display_type,
            Duration::from_millis(cfg.poll_interval_ms),
        ),
    );

    let eng = engine::Engine::new(cfg);
    log::info!("Starting engine");
    eng.run(any_disp, source, wifi_handle);
}
