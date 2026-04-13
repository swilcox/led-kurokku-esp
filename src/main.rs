use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use std::time::Duration;

mod config;
mod display;
mod engine;
mod font;
mod framebuf;
mod network;
mod ota;
mod widget;
mod wifi;

fn main() {
    // Initialize ESP-IDF logging and system services
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("LED Kurokku ESP starting up...");

    let peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take().unwrap();
    let nvs_partition = EspDefaultNvsPartition::take().unwrap();

    // Load config from NVS (falls back to compile-time env vars)
    // Clone the partition handle — NVS partition is shared, WiFi needs one too
    let nvs = config::open_nvs(nvs_partition.clone()).expect("Failed to open NVS");
    let cfg = config::AppConfig::load(&nvs);
    drop(nvs); // release the NVS handle before WiFi needs the partition

    #[cfg(feature = "max7219")]
    {
        use display::{AnyDisplay, Display, PixelDisplay};
        use widget::{CancelToken, Widget};

        // Initialize display
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

        let mut any_disp = AnyDisplay::Pixel(Box::new(disp));

        // Show startup message
        {
            let cancel = CancelToken::with_timeout(Duration::from_secs(2));
            let mut status = widget::status::Status::new("KUROKKU", Duration::from_secs(2));
            let _ = status.run(&mut any_disp, &cancel);
        }

        // Check WiFi config
        if !cfg.has_wifi_config() {
            log::warn!("No WiFi credentials configured");
            let cancel = CancelToken::with_timeout(Duration::from_secs(3));
            let mut status = widget::status::Status::new("NO CFG", Duration::from_secs(3));
            let _ = status.run(&mut any_disp, &cancel);
        }

        // Connect to WiFi
        let _wifi = if cfg.has_wifi_config() {
            match wifi::connect(
                &cfg.wifi_ssid,
                &cfg.wifi_password,
                peripherals.modem,
                sysloop,
                nvs_partition,
            ) {
                Ok(w) => {
                    let ip = wifi::get_ip(&w).unwrap_or_else(|| "?.?.?.?".to_string());
                    log::info!("Connected: {}", ip);
                    {
                        let cancel = CancelToken::with_timeout(Duration::from_secs(3));
                        let mut status = widget::status::Status::new(&ip, Duration::from_secs(3));
                        let _ = status.run(&mut any_disp, &cancel);
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

                    Some(w)
                }
                Err(e) => {
                    log::warn!("WiFi failed: {}", e);
                    {
                        let cancel = CancelToken::with_timeout(Duration::from_secs(3));
                        let mut status =
                            widget::status::Status::new("NO WIFI", Duration::from_secs(3));
                        let _ = status.run(&mut any_disp, &cancel);
                    }
                    None
                }
            }
        } else {
            None
        };

        // Build instruction source
        let display_type = "max7219";
        let source: Box<dyn network::InstructionSource> = Box::new(
            network::polling::HttpPoller::new(
                &cfg.server_url,
                &cfg.device_id,
                display_type,
                Duration::from_millis(cfg.poll_interval_ms),
            ),
        );

        // Run the engine (blocks forever)
        let eng = engine::Engine::new(cfg);
        log::info!("Starting engine");
        eng.run(any_disp, source);
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
