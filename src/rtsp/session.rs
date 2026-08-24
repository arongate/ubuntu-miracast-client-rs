//! RTSP/WFD session state machine for Miracast source role.
//!
//! Implements M1-M7 session establishment, M8-M9 teardown, and M14 keep-alive
//! per Wi-Fi Display Technical Specification v2.3 §4.5.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{info, warn};

use super::messages::{ParsedMessage, RtspMethod, RtspRequest, RtspResponse, try_parse_message};
use crate::wfd::WfdParameters;

/// Default WFD control port (TCP).
pub const WFD_DEFAULT_CONTROL_PORT: u16 = 7236;

/// Session timeout (WFD spec default: 30s).
const SESSION_TIMEOUT: Duration = Duration::from_secs(30);

/// Keep-alive interval (must be less than session timeout).
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);

/// Response timeout.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum receive buffer size.
const MAX_RECV_BUFFER: usize = 65536;

/// WFD RTSP session states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Init,
    M1Sent,
    M2Received,
    M3Sent,
    M4Sent,
    M5Sent,
    M6Received,
    M7Received,
    Streaming,
    Teardown,
    Done,
    Error,
}

/// Configuration for an RTSP/WFD session.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub peer_ip: String,
    pub control_port: u16,
    pub local_ip: String,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            peer_ip: String::new(),
            control_port: WFD_DEFAULT_CONTROL_PORT,
            local_ip: String::new(),
        }
    }
}

/// Parameters negotiated during session establishment.
#[derive(Debug, Clone, Default)]
pub struct NegotiatedParams {
    pub sink_capabilities: Option<WfdParameters>,
    pub rtp_port: u16,
    pub session_id: String,
    pub selected_width: u32,
    pub selected_height: u32,
    pub selected_fps: u32,
}

/// Manages an RTSP/WFD session with a Miracast sink.
pub struct RtspSession {
    config: SessionConfig,
    state: SessionState,
    cseq: u32,
    stream: Option<TcpStream>,
    recv_buffer: Vec<u8>,
    pub negotiated: NegotiatedParams,
}

impl RtspSession {
    /// Create a new RTSP session.
    pub fn new(config: SessionConfig) -> Self {
        Self {
            config,
            state: SessionState::Init,
            cseq: 0,
            stream: None,
            recv_buffer: Vec::with_capacity(4096),
            negotiated: NegotiatedParams::default(),
        }
    }

    /// Current session state.
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Whether the session is actively streaming.
    pub fn is_established(&self) -> bool {
        self.state == SessionState::Streaming
    }

    /// Establish the RTSP/WFD session (M1-M7).
    ///
    /// This is an async operation that connects to the sink and drives
    /// the full WFD session negotiation.
    pub async fn establish(&mut self) -> anyhow::Result<()> {
        self.connect().await?;
        self.do_m1_options().await?;
        self.handle_m2_options().await?;
        self.do_m3_get_parameter().await?;
        self.do_m4_set_parameter().await?;
        self.do_m5_trigger_setup().await?;
        self.handle_m6_setup().await?;
        self.handle_m7_play().await?;
        self.state = SessionState::Streaming;
        info!("RTSP session established — ready to stream");
        Ok(())
    }

    /// Tear down the session gracefully (M8-M9).
    pub async fn teardown(&mut self) -> anyhow::Result<()> {
        if self.state != SessionState::Streaming {
            return Ok(());
        }

        self.state = SessionState::Teardown;

        // M8: Send trigger TEARDOWN
        let cseq = self.next_cseq();
        let body = "wfd_trigger_method: TEARDOWN";
        let req = RtspRequest::new(RtspMethod::SetParameter, &self.presentation_url())
            .with_cseq(cseq)
            .with_header("Session", &self.negotiated.session_id)
            .with_body(body);
        self.send(&req.serialize()).await?;

        // Try to get response (non-critical if it fails)
        let _ = timeout(Duration::from_secs(3), self.recv_response()).await;

        self.state = SessionState::Done;
        self.cleanup().await;
        info!("RTSP session torn down");
        Ok(())
    }

    /// Send a keep-alive (M14: GET_PARAMETER with no body).
    pub async fn send_keepalive(&mut self) -> anyhow::Result<()> {
        let cseq = self.next_cseq();
        let req = RtspRequest::new(RtspMethod::GetParameter, &self.presentation_url())
            .with_cseq(cseq)
            .with_header("Session", &self.negotiated.session_id);
        self.send(&req.serialize()).await?;
        let resp = self.recv_response().await?;
        if resp.status_code != 200 {
            warn!("Keep-alive failed: {}", resp.status_code);
        }
        Ok(())
    }

    // ─── Session establishment steps ──────────────────────────────────────

    async fn connect(&mut self) -> anyhow::Result<()> {
        let addr = format!("{}:{}", self.config.peer_ip, self.config.control_port);
        info!("Connecting to RTSP sink at {addr}");

        let stream = timeout(Duration::from_secs(10), TcpStream::connect(&addr))
            .await
            .map_err(|_| anyhow::anyhow!("Connection timeout to {addr}"))??;

        self.stream = Some(stream);
        info!("TCP connection established to {addr}");
        Ok(())
    }

    async fn do_m1_options(&mut self) -> anyhow::Result<()> {
        let cseq = self.next_cseq();
        let req = RtspRequest::new(RtspMethod::Options, "*")
            .with_cseq(cseq)
            .with_header("Require", "org.wfa.wfd1.0");
        self.send(&req.serialize()).await?;
        self.state = SessionState::M1Sent;

        let resp = self.recv_response().await?;
        if resp.status_code != 200 {
            anyhow::bail!("M1 OPTIONS rejected: {} {}", resp.status_code, resp.reason);
        }

        let public = resp.headers.get("Public").cloned().unwrap_or_default();
        info!("M1: Sink supports: {public}");
        Ok(())
    }

    async fn handle_m2_options(&mut self) -> anyhow::Result<()> {
        let req = self.recv_request().await?;
        if req.method != RtspMethod::Options {
            anyhow::bail!("Expected M2 OPTIONS, got {}", req.method);
        }

        let resp = RtspResponse::ok(req.cseq().unwrap_or(0))
            .with_header("Public", "org.wfa.wfd1.0, GET_PARAMETER, SET_PARAMETER");
        self.send(&resp.serialize()).await?;
        self.state = SessionState::M2Received;
        info!("M2: Responded to sink OPTIONS");
        Ok(())
    }

    async fn do_m3_get_parameter(&mut self) -> anyhow::Result<()> {
        let cseq = self.next_cseq();
        let body = "wfd_video_formats\r\nwfd_audio_codecs\r\nwfd_client_rtp_ports\r\nwfd_content_protection";
        let req = RtspRequest::new(RtspMethod::GetParameter, "rtsp://localhost/wfd1.0")
            .with_cseq(cseq)
            .with_body(body);
        self.send(&req.serialize()).await?;
        self.state = SessionState::M3Sent;

        let resp = self.recv_response().await?;
        if resp.status_code != 200 {
            anyhow::bail!("M3 GET_PARAMETER rejected: {}", resp.status_code);
        }

        let sink_params = WfdParameters::parse_body(&resp.body);
        info!(
            "M3: Sink RTP port = {}",
            sink_params.client_rtp_port.unwrap_or(19000)
        );
        self.negotiated.sink_capabilities = Some(sink_params);
        Ok(())
    }

    async fn do_m4_set_parameter(&mut self) -> anyhow::Result<()> {
        let rtp_port = self
            .negotiated
            .sink_capabilities
            .as_ref()
            .and_then(|c| c.client_rtp_port)
            .unwrap_or(19000);

        let presentation_url = self.presentation_url();
        let body = format!(
            "wfd_video_formats: 00 00 01 02 000000A1 00000000 00000000 00 0000 0000 00 none none\r\n\
             wfd_audio_codecs: LPCM 00000003 00\r\n\
             wfd_client_rtp_ports: RTP/AVP/UDP;unicast {rtp_port} 0 mode=play\r\n\
             wfd_presentation_URL: {presentation_url} none"
        );

        let cseq = self.next_cseq();
        let req = RtspRequest::new(RtspMethod::SetParameter, "rtsp://localhost/wfd1.0")
            .with_cseq(cseq)
            .with_body(&body);
        self.send(&req.serialize()).await?;
        self.state = SessionState::M4Sent;

        let resp = self.recv_response().await?;
        if resp.status_code != 200 {
            anyhow::bail!("M4 SET_PARAMETER rejected: {}", resp.status_code);
        }
        info!("M4: Session parameters accepted");
        Ok(())
    }

    async fn do_m5_trigger_setup(&mut self) -> anyhow::Result<()> {
        let cseq = self.next_cseq();
        let req = RtspRequest::new(RtspMethod::SetParameter, "rtsp://localhost/wfd1.0")
            .with_cseq(cseq)
            .with_body("wfd_trigger_method: SETUP");
        self.send(&req.serialize()).await?;
        self.state = SessionState::M5Sent;

        let resp = self.recv_response().await?;
        if resp.status_code != 200 {
            anyhow::bail!("M5 trigger SETUP rejected: {}", resp.status_code);
        }
        info!("M5: Trigger SETUP accepted");
        Ok(())
    }

    async fn handle_m6_setup(&mut self) -> anyhow::Result<()> {
        let req = self.recv_request().await?;
        if req.method != RtspMethod::Setup {
            anyhow::bail!("Expected M6 SETUP, got {}", req.method);
        }

        // Parse transport header for client port
        let transport = req.headers.get("Transport").cloned().unwrap_or_default();
        let rtp_port = parse_transport_port(&transport);
        self.negotiated.rtp_port = rtp_port;

        // Generate session ID
        let session_id = uuid::Uuid::new_v4().to_string()[..16].to_string();
        self.negotiated.session_id = session_id.clone();

        let resp = RtspResponse::ok(req.cseq().unwrap_or(0))
            .with_header("Session", &format!("{session_id};timeout=30"))
            .with_header(
                "Transport",
                &format!("RTP/AVP/UDP;unicast;client_port={rtp_port};server_port={rtp_port}"),
            );
        self.send(&resp.serialize()).await?;
        self.state = SessionState::M6Received;
        info!("M6: SETUP complete, RTP port={rtp_port}");
        Ok(())
    }

    async fn handle_m7_play(&mut self) -> anyhow::Result<()> {
        let req = self.recv_request().await?;
        if req.method != RtspMethod::Play {
            anyhow::bail!("Expected M7 PLAY, got {}", req.method);
        }

        let resp = RtspResponse::ok(req.cseq().unwrap_or(0))
            .with_header("Session", &self.negotiated.session_id);
        self.send(&resp.serialize()).await?;
        self.state = SessionState::M7Received;
        info!("M7: PLAY received, streaming can begin");
        Ok(())
    }

    // ─── Network I/O ──────────────────────────────────────────────────────

    async fn send(&mut self, data: &[u8]) -> anyhow::Result<()> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))?;
        stream.write_all(data).await?;
        stream.flush().await?;
        Ok(())
    }

    async fn recv_response(&mut self) -> anyhow::Result<RtspResponse> {
        let msg = timeout(RESPONSE_TIMEOUT, self.recv_message())
            .await
            .map_err(|_| anyhow::anyhow!("Timeout waiting for RTSP response"))??;

        match msg {
            ParsedMessage::Response(resp) => Ok(resp),
            ParsedMessage::Request(req) => {
                anyhow::bail!("Expected response, got {} request", req.method)
            }
        }
    }

    async fn recv_request(&mut self) -> anyhow::Result<RtspRequest> {
        let msg = timeout(RESPONSE_TIMEOUT, self.recv_message())
            .await
            .map_err(|_| anyhow::anyhow!("Timeout waiting for RTSP request"))??;

        match msg {
            ParsedMessage::Request(req) => Ok(req),
            ParsedMessage::Response(resp) => {
                anyhow::bail!("Expected request, got {} response", resp.status_code)
            }
        }
    }

    async fn recv_message(&mut self) -> anyhow::Result<ParsedMessage> {
        loop {
            // Try to parse from existing buffer
            if let Some((msg, consumed)) = try_parse_message(&self.recv_buffer)? {
                self.recv_buffer.drain(..consumed);
                return Ok(msg);
            }

            // Read more data
            if self.recv_buffer.len() >= MAX_RECV_BUFFER {
                anyhow::bail!("Receive buffer overflow — possible attack");
            }

            let stream = self
                .stream
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Not connected"))?;

            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).await?;
            if n == 0 {
                anyhow::bail!("Connection closed by peer");
            }
            self.recv_buffer.extend_from_slice(&buf[..n]);
        }
    }

    // ─── Helpers ──────────────────────────────────────────────────────────

    fn next_cseq(&mut self) -> u32 {
        self.cseq += 1;
        self.cseq
    }

    fn presentation_url(&self) -> String {
        format!("rtsp://{}/wfd1.0/streamid=0", self.config.local_ip)
    }

    async fn cleanup(&mut self) {
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        self.recv_buffer.clear();
    }
}

/// Parse client_port from RTSP Transport header.
fn parse_transport_port(transport: &str) -> u16 {
    for param in transport.split(';') {
        let param = param.trim();
        if let Some(port_str) = param.strip_prefix("client_port=") {
            // May be "port" or "port-port" range
            let first = port_str.split('-').next().unwrap_or(port_str);
            if let Ok(port) = first.parse::<u16>() {
                if port > 0 {
                    return port;
                }
            }
        }
    }
    19000 // Default fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_transport_port() {
        assert_eq!(
            parse_transport_port("RTP/AVP/UDP;unicast;client_port=19000"),
            19000
        );
        assert_eq!(
            parse_transport_port("RTP/AVP/UDP;unicast;client_port=5004-5005"),
            5004
        );
        assert_eq!(parse_transport_port("RTP/AVP/UDP;unicast"), 19000);
    }

    #[test]
    fn test_session_config_default() {
        let config = SessionConfig::default();
        assert_eq!(config.control_port, WFD_DEFAULT_CONTROL_PORT);
    }
}
