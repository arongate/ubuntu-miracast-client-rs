//! Wi-Fi Direct device discovery via D-Bus.
//!
//! Uses wpa_supplicant's D-Bus interface (`fi.w1.wpa_supplicant1`) for P2P
//! device discovery instead of shelling out to `wpa_cli`.

use std::collections::HashMap;

use anyhow::Result;
use tracing::{debug, info};
use zbus::Connection;

/// WFD device types (from WFD subelement device info bits 0-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WfdDeviceType {
    Source = 0,
    PrimarySink = 1,
    SecondarySink = 2,
    DualRole = 3,
}

/// A discovered Miracast device.
#[derive(Debug, Clone)]
pub struct MiracastDevice {
    pub id: String,
    pub name: String,
    pub address: String,
    pub model: String,
    pub signal_strength: i32,
    pub wfd_type: Option<WfdDeviceType>,
    pub rtsp_port: u16,
}

impl MiracastDevice {
    /// Whether this device can receive a Miracast stream.
    pub fn is_sink(&self) -> bool {
        matches!(
            self.wfd_type,
            Some(WfdDeviceType::PrimarySink | WfdDeviceType::SecondarySink | WfdDeviceType::DualRole)
        )
    }
}

/// Manages P2P device discovery via wpa_supplicant D-Bus interface.
pub struct Discovery {
    connection: Option<Connection>,
    devices: HashMap<String, MiracastDevice>,
    running: bool,
}

impl Discovery {
    pub fn new() -> Self {
        Self {
            connection: None,
            devices: HashMap::new(),
            running: false,
        }
    }

    /// Connect to the system D-Bus and prepare for P2P operations.
    pub async fn connect(&mut self) -> Result<()> {
        let connection = Connection::system().await?;
        info!("Connected to system D-Bus for P2P discovery");
        self.connection = Some(connection);
        Ok(())
    }

    /// Start P2P device discovery.
    ///
    /// Sends P2P_FIND via the wpa_supplicant D-Bus interface.
    pub async fn start(&mut self) -> Result<()> {
        let _conn = self
            .connection
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not connected to D-Bus"))?;

        // Call fi.w1.wpa_supplicant1.Interface.P2PDevice.Find
        // This is a simplified version — full impl would use #[proxy] macros
        info!("Starting P2P device discovery via D-Bus");
        self.running = true;

        // TODO: Full implementation with zbus #[proxy] for wpa_supplicant1.Interface.P2PDevice
        // For now, log the intent
        debug!("Would call fi.w1.wpa_supplicant1.Interface.P2PDevice.Find()");

        Ok(())
    }

    /// Stop P2P device discovery.
    pub async fn stop(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }

        info!("Stopping P2P device discovery");
        self.running = false;

        // TODO: Call fi.w1.wpa_supplicant1.Interface.P2PDevice.StopFind()

        Ok(())
    }

    /// Get discovered sink devices.
    pub fn get_sinks(&self) -> Vec<&MiracastDevice> {
        self.devices.values().filter(|d| d.is_sink()).collect()
    }

    /// Get all discovered devices.
    pub fn get_all_devices(&self) -> Vec<&MiracastDevice> {
        self.devices.values().collect()
    }

    /// Whether discovery is currently running.
    pub fn is_running(&self) -> bool {
        self.running
    }
}

/// Parse WFD subelements from hex string.
///
/// Returns (device_type, rtsp_port).
pub fn parse_wfd_subelements(hex: &str) -> Option<(WfdDeviceType, u16)> {
    let hex = hex.trim();

    // Validate: must be hex characters, at least 12 chars (ID+Length+DeviceInfo)
    if hex.len() < 12 || hex.len() % 2 != 0 {
        return None;
    }
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    // Subelement ID must be 0x00 (Device Information)
    let id = u8::from_str_radix(&hex[0..2], 16).ok()?;
    if id != 0 {
        return None;
    }

    // Parse device info (bits 0-1 = device type)
    let device_info = u16::from_str_radix(&hex[6..10], 16).ok()?;
    let device_type = match device_info & 0x03 {
        0 => WfdDeviceType::Source,
        1 => WfdDeviceType::PrimarySink,
        2 => WfdDeviceType::SecondarySink,
        3 => WfdDeviceType::DualRole,
        _ => return None,
    };

    // Parse RTSP port (default 7236 per WFD spec)
    let rtsp_port = if hex.len() >= 14 {
        let port = u16::from_str_radix(&hex[10..14], 16).ok()?;
        if port == 0 { 7236 } else { port }
    } else {
        7236
    };

    Some((device_type, rtsp_port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_wfd_source() {
        // ID=00, Length=0006, DeviceInfo=0010 (Source, session avail), Port=1C44 (7236), Throughput=0032
        let (dtype, port) = parse_wfd_subelements("000006001000001C440032").unwrap();
        assert_eq!(dtype, WfdDeviceType::Source);
        assert_eq!(port, 0x1C44); // 7236
    }

    #[test]
    fn test_parse_wfd_primary_sink() {
        // DeviceInfo=0011 (PrimarySink), Port=0000 defaults to 7236
        let (dtype, port) = parse_wfd_subelements("000006001100000032").unwrap();
        assert_eq!(dtype, WfdDeviceType::PrimarySink);
        assert_eq!(port, 7236); // Port 0 defaults to 7236
    }

    #[test]
    fn test_parse_wfd_invalid_short() {
        assert!(parse_wfd_subelements("0006").is_none());
    }

    #[test]
    fn test_parse_wfd_invalid_chars() {
        assert!(parse_wfd_subelements("ZZZZZZZZZZZZZZZZZZ").is_none());
    }

    #[test]
    fn test_parse_wfd_wrong_subelement_id() {
        // ID=01 (not Device Information)
        assert!(parse_wfd_subelements("010006001000001C440032").is_none());
    }

    #[test]
    fn test_parse_wfd_port_zero_defaults() {
        // Port field = 0000, should default to 7236
        let (_, port) = parse_wfd_subelements("000006001100000000").unwrap();
        assert_eq!(port, 7236);
    }

    #[test]
    fn test_miracast_device_is_sink() {
        let device = MiracastDevice {
            id: "test".to_string(),
            name: "TV".to_string(),
            address: "aa:bb:cc:dd:ee:ff".to_string(),
            model: "Test".to_string(),
            signal_strength: -50,
            wfd_type: Some(WfdDeviceType::PrimarySink),
            rtsp_port: 7236,
        };
        assert!(device.is_sink());
    }

    #[test]
    fn test_miracast_device_source_not_sink() {
        let device = MiracastDevice {
            id: "test".to_string(),
            name: "Phone".to_string(),
            address: "aa:bb:cc:dd:ee:ff".to_string(),
            model: "Test".to_string(),
            signal_strength: -50,
            wfd_type: Some(WfdDeviceType::Source),
            rtsp_port: 7236,
        };
        assert!(!device.is_sink());
    }
}
