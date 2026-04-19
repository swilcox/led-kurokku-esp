use anyhow::Result;
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use esp_idf_svc::http::Method;
use esp_idf_svc::ota::EspOta;
use log;
use std::time::Duration;

fn http_config() -> HttpConfig {
    HttpConfig {
        timeout: Some(Duration::from_secs(60)),
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        // GitHub and CDN responses have large headers; 512-byte default overflows.
        buffer_size: Some(4096),
        buffer_size_tx: Some(1024),
        ..Default::default()
    }
}

/// Follow up to one HTTP redirect, returning the final URL.
/// GitHub release download URLs redirect to objects.githubusercontent.com.
fn resolve_url(url: &str) -> Result<String> {
    let mut conn = EspHttpConnection::new(&http_config())
        .map_err(|e| anyhow::anyhow!("OTA HTTP connection failed: {}", e))?;

    conn.initiate_request(Method::Get, url, &[])
        .map_err(|e| anyhow::anyhow!("OTA HTTP request failed: {}", e))?;

    conn.initiate_response()
        .map_err(|e| anyhow::anyhow!("OTA HTTP response failed: {}", e))?;

    let status = conn.status();
    if status == 301 || status == 302 || status == 303 || status == 307 || status == 308 {
        let location = conn
            .header("Location")
            .ok_or_else(|| anyhow::anyhow!("OTA redirect (status {}) but no Location header", status))?
            .to_string();
        log::info!("OTA: redirect {} -> {}", status, location);
        return Ok(location);
    }

    Ok(url.to_string())
}

/// Download firmware from the given URL and flash it to the inactive OTA partition.
/// Reboots the device on success.
pub fn perform_ota(url: &str) -> Result<()> {
    log::info!("OTA: starting download from {}", url);

    let final_url = resolve_url(url)?;

    let mut connection = EspHttpConnection::new(&http_config())
        .map_err(|e| anyhow::anyhow!("OTA HTTP connection failed: {}", e))?;

    connection
        .initiate_request(Method::Get, &final_url, &[])
        .map_err(|e| anyhow::anyhow!("OTA HTTP request failed: {}", e))?;

    connection
        .initiate_response()
        .map_err(|e| anyhow::anyhow!("OTA HTTP response failed: {}", e))?;

    let status = connection.status();
    if status != 200 {
        anyhow::bail!("OTA server returned status {}", status);
    }

    let mut ota = EspOta::new()
        .map_err(|e| anyhow::anyhow!("OTA init failed: {}", e))?;

    let mut update = ota
        .initiate_update()
        .map_err(|e| anyhow::anyhow!("OTA initiate update failed: {}", e))?;

    let mut buf = [0u8; 4096];
    let mut total_bytes: usize = 0;

    loop {
        let bytes_read = connection
            .read(&mut buf)
            .map_err(|e| anyhow::anyhow!("OTA read failed at {} bytes: {}", total_bytes, e))?;

        if bytes_read == 0 {
            break;
        }

        update
            .write(&buf[..bytes_read])
            .map_err(|e| anyhow::anyhow!("OTA write failed at {} bytes: {}", total_bytes, e))?;

        total_bytes += bytes_read;

        if total_bytes % (64 * 1024) < 4096 {
            log::info!("OTA: {}KB downloaded", total_bytes / 1024);
        }
    }

    log::info!("OTA: download complete, {} bytes total. Finalizing...", total_bytes);

    update
        .complete()
        .map_err(|e| anyhow::anyhow!("OTA complete failed: {}", e))?;

    log::info!("OTA: update complete, rebooting...");
    esp_idf_svc::hal::reset::restart();
}

/// Get information about the currently running firmware.
pub fn running_partition_info() -> Result<String> {
    let ota = EspOta::new()
        .map_err(|e| anyhow::anyhow!("OTA init failed: {}", e))?;

    let running = ota.get_running_slot()
        .map_err(|e| anyhow::anyhow!("Failed to get running slot: {}", e))?;

    Ok(format!(
        "partition={}, firmware={}",
        running.label,
        running.firmware.as_ref().map(|f| f.version.as_str()).unwrap_or("unknown"),
    ))
}
