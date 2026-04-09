use anyhow::Result;
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use std::time::Duration;

use super::{InstructionSource, ServerResponse};

pub struct HttpPoller {
    base_url: String,
    device_id: String,
    display_type: String,
    poll_interval: Duration,
}

impl HttpPoller {
    pub fn new(base_url: &str, device_id: &str, display_type: &str, poll_interval: Duration) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            device_id: device_id.to_string(),
            display_type: display_type.to_string(),
            poll_interval,
        }
    }

    pub fn set_poll_interval(&mut self, interval: Duration) {
        self.poll_interval = interval;
    }
}

impl InstructionSource for HttpPoller {
    fn next_instruction(&mut self) -> Result<Option<ServerResponse>> {
        let url = format!(
            "{}/api/v1/devices/{}/instruction?display_type={}",
            self.base_url, self.device_id, self.display_type
        );

        let mut connection = EspHttpConnection::new(&HttpConfig {
            timeout: Some(Duration::from_secs(10)),
            ..Default::default()
        })
        .map_err(|e| anyhow::anyhow!("HTTP connection failed: {}", e))?;

        connection
            .initiate_request(esp_idf_svc::http::Method::Get, &url, &[])
            .map_err(|e| anyhow::anyhow!("HTTP initiate failed: {}", e))?;

        connection
            .initiate_response()
            .map_err(|e| anyhow::anyhow!("HTTP response failed: {}", e))?;

        let status = connection.status();
        if status != 200 {
            anyhow::bail!("Server returned status {}", status);
        }

        // Read response body
        let mut buf = [0u8; 2048];
        let mut body = Vec::new();
        loop {
            let bytes_read = connection
                .read(&mut buf)
                .map_err(|e| anyhow::anyhow!("HTTP read failed: {}", e))?;
            if bytes_read == 0 {
                break;
            }
            body.extend_from_slice(&buf[..bytes_read]);
        }

        let resp: ServerResponse = serde_json::from_slice(&body)?;

        // Update poll interval if server requested it
        if let Some(interval_ms) = resp.poll_interval_ms {
            self.poll_interval = Duration::from_millis(interval_ms);
        }

        Ok(Some(resp))
    }

    fn recommended_interval(&self) -> Duration {
        self.poll_interval
    }
}
