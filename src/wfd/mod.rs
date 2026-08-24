//! Wi-Fi Display (WFD) parameter parsing and formatting.
//!
//! Reference: Wi-Fi Display Technical Specification v2.3 §4.5

use serde::{Deserialize, Serialize};

/// Composite WFD parameters exchanged in M3/M4.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WfdParameters {
    pub video_formats: Option<String>,
    pub audio_codecs: Option<String>,
    pub client_rtp_port: Option<u16>,
    pub content_protection: Option<String>,
    pub presentation_url: Option<String>,
    pub trigger_method: Option<String>,
}

impl WfdParameters {
    /// Parse WFD parameters from an RTSP message body.
    pub fn parse_body(body: &str) -> Self {
        let mut params = Self::default();

        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim().to_lowercase();
                let value = value.trim();

                match key.as_str() {
                    "wfd_video_formats" => params.video_formats = Some(value.to_string()),
                    "wfd_audio_codecs" => params.audio_codecs = Some(value.to_string()),
                    "wfd_client_rtp_ports" => {
                        params.client_rtp_port = parse_rtp_port(value);
                    }
                    "wfd_content_protection" => {
                        params.content_protection = Some(value.to_string());
                    }
                    "wfd_presentation_url" => {
                        params.presentation_url = Some(value.to_string());
                    }
                    "wfd_trigger_method" => {
                        params.trigger_method = Some(value.to_string());
                    }
                    _ => {}
                }
            }
        }

        params
    }
}

/// Parse the RTP port from a wfd_client_rtp_ports value.
/// Format: "RTP/AVP/UDP;unicast 19000 0 mode=play"
fn parse_rtp_port(value: &str) -> Option<u16> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() >= 2 {
        parts[1].parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_m3_response_body() {
        let body = "wfd_video_formats: 00 00 01 02 000000A1 00000000 00000000 00 0000 0000 00 none none\r\n\
                    wfd_audio_codecs: LPCM 00000003 00\r\n\
                    wfd_client_rtp_ports: RTP/AVP/UDP;unicast 19000 0 mode=play\r\n\
                    wfd_content_protection: none";
        let params = WfdParameters::parse_body(body);
        assert!(params.video_formats.is_some());
        assert!(params.audio_codecs.is_some());
        assert_eq!(params.client_rtp_port, Some(19000));
        assert_eq!(params.content_protection.as_deref(), Some("none"));
    }

    #[test]
    fn test_parse_trigger_method() {
        let body = "wfd_trigger_method: SETUP";
        let params = WfdParameters::parse_body(body);
        assert_eq!(params.trigger_method.as_deref(), Some("SETUP"));
    }

    #[test]
    fn test_parse_empty_body() {
        let params = WfdParameters::parse_body("");
        assert!(params.video_formats.is_none());
        assert!(params.client_rtp_port.is_none());
    }
}
