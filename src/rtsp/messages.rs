//! RTSP 1.0 message parser and builder (RFC 2326).
//!
//! Implements the subset of RTSP required by Wi-Fi Display Technical Specification v2.3.
//! The WFD protocol uses RTSP 1.0 (NOT RTSP 2.0 / RFC 7826).

use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

/// Maximum message size to prevent DoS (64 KB).
const MAX_MESSAGE_SIZE: usize = 65536;

/// Maximum Content-Length we'll accept.
const MAX_CONTENT_LENGTH: usize = 16384;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("empty message")]
    Empty,
    #[error("invalid request line: {0}")]
    InvalidRequestLine(String),
    #[error("invalid status line: {0}")]
    InvalidStatusLine(String),
    #[error("unsupported RTSP version: {0}")]
    UnsupportedVersion(String),
    #[error("unknown RTSP method: {0}")]
    UnknownMethod(String),
    #[error("invalid status code: {0}")]
    InvalidStatusCode(String),
    #[error("message too large: {0} bytes")]
    MessageTooLarge(usize),
    #[error("content-length exceeds maximum: {0}")]
    ContentLengthExceeded(usize),
    #[error("incomplete message")]
    Incomplete,
}

/// RTSP methods used in Wi-Fi Display session management.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RtspMethod {
    Options,
    GetParameter,
    SetParameter,
    Setup,
    Play,
    Pause,
    Teardown,
}

impl RtspMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Options => "OPTIONS",
            Self::GetParameter => "GET_PARAMETER",
            Self::SetParameter => "SET_PARAMETER",
            Self::Setup => "SETUP",
            Self::Play => "PLAY",
            Self::Pause => "PAUSE",
            Self::Teardown => "TEARDOWN",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, ParseError> {
        match s {
            "OPTIONS" => Ok(Self::Options),
            "GET_PARAMETER" => Ok(Self::GetParameter),
            "SET_PARAMETER" => Ok(Self::SetParameter),
            "SETUP" => Ok(Self::Setup),
            "PLAY" => Ok(Self::Play),
            "PAUSE" => Ok(Self::Pause),
            "TEARDOWN" => Ok(Self::Teardown),
            _ => Err(ParseError::UnknownMethod(s.to_string())),
        }
    }
}

impl fmt::Display for RtspMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An RTSP request message.
#[derive(Debug, Clone)]
pub struct RtspRequest {
    pub method: RtspMethod,
    pub uri: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl RtspRequest {
    /// Create a new request with the given method and URI.
    pub fn new(method: RtspMethod, uri: &str) -> Self {
        Self {
            method,
            uri: uri.to_string(),
            headers: HashMap::new(),
            body: String::new(),
        }
    }

    /// Set the CSeq header.
    pub fn with_cseq(mut self, cseq: u32) -> Self {
        self.headers.insert("CSeq".to_string(), cseq.to_string());
        self
    }

    /// Set a header.
    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }

    /// Set the body.
    pub fn with_body(mut self, body: &str) -> Self {
        self.body = body.to_string();
        self
    }

    /// Get the CSeq header value.
    pub fn cseq(&self) -> Option<u32> {
        self.headers.get("CSeq")?.parse().ok()
    }

    /// Get the Session header value (without timeout parameter).
    pub fn session_id(&self) -> Option<&str> {
        self.headers
            .get("Session")
            .map(|s| s.split(';').next().unwrap_or(s).trim())
    }

    /// Serialize to bytes for transmission.
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = String::new();
        out.push_str(&format!("{} {} RTSP/1.0\r\n", self.method, self.uri));

        let mut headers = self.headers.clone();
        if !self.body.is_empty() {
            headers.insert("Content-Length".to_string(), self.body.len().to_string());
            headers
                .entry("Content-Type".to_string())
                .or_insert_with(|| "text/parameters".to_string());
        }

        for (key, value) in &headers {
            out.push_str(&format!("{key}: {value}\r\n"));
        }
        out.push_str("\r\n");

        if !self.body.is_empty() {
            out.push_str(&self.body);
        }

        out.into_bytes()
    }

    /// Parse from bytes.
    pub fn parse(data: &[u8]) -> Result<Self, ParseError> {
        if data.is_empty() {
            return Err(ParseError::Empty);
        }
        if data.len() > MAX_MESSAGE_SIZE {
            return Err(ParseError::MessageTooLarge(data.len()));
        }

        let text = String::from_utf8_lossy(data);
        let (header_section, body) = split_header_body(&text);

        let mut lines = header_section.lines();
        let request_line = lines.next().ok_or(ParseError::Empty)?;

        // Parse: "METHOD URI RTSP/1.0"
        let parts: Vec<&str> = request_line.splitn(3, ' ').collect();
        if parts.len() != 3 {
            return Err(ParseError::InvalidRequestLine(request_line.to_string()));
        }

        let method = RtspMethod::from_str(parts[0])?;
        let uri = parts[1].to_string();

        if parts[2] != "RTSP/1.0" {
            return Err(ParseError::UnsupportedVersion(parts[2].to_string()));
        }

        let headers = parse_headers(lines);

        Ok(Self {
            method,
            uri,
            headers,
            body: body.to_string(),
        })
    }
}

/// An RTSP response message.
#[derive(Debug, Clone)]
pub struct RtspResponse {
    pub status_code: u16,
    pub reason: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl RtspResponse {
    /// Create a 200 OK response.
    pub fn ok(cseq: u32) -> Self {
        let mut headers = HashMap::new();
        headers.insert("CSeq".to_string(), cseq.to_string());
        Self {
            status_code: 200,
            reason: "OK".to_string(),
            headers,
            body: String::new(),
        }
    }

    /// Create an error response.
    pub fn error(status_code: u16, cseq: u32) -> Self {
        let reason = status_reason(status_code).to_string();
        let mut headers = HashMap::new();
        headers.insert("CSeq".to_string(), cseq.to_string());
        Self {
            status_code,
            reason,
            headers,
            body: String::new(),
        }
    }

    /// Set a header.
    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }

    /// Set the body.
    pub fn with_body(mut self, body: &str) -> Self {
        self.body = body.to_string();
        self
    }

    /// Get the CSeq value.
    pub fn cseq(&self) -> Option<u32> {
        self.headers.get("CSeq")?.parse().ok()
    }

    /// Get the Session header value.
    pub fn session_id(&self) -> Option<&str> {
        self.headers
            .get("Session")
            .map(|s| s.split(';').next().unwrap_or(s).trim())
    }

    /// Serialize to bytes for transmission.
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = String::new();
        out.push_str(&format!(
            "RTSP/1.0 {} {}\r\n",
            self.status_code, self.reason
        ));

        let mut headers = self.headers.clone();
        if !self.body.is_empty() {
            headers.insert("Content-Length".to_string(), self.body.len().to_string());
            headers
                .entry("Content-Type".to_string())
                .or_insert_with(|| "text/parameters".to_string());
        }

        for (key, value) in &headers {
            out.push_str(&format!("{key}: {value}\r\n"));
        }
        out.push_str("\r\n");

        if !self.body.is_empty() {
            out.push_str(&self.body);
        }

        out.into_bytes()
    }

    /// Parse from bytes.
    pub fn parse(data: &[u8]) -> Result<Self, ParseError> {
        if data.is_empty() {
            return Err(ParseError::Empty);
        }
        if data.len() > MAX_MESSAGE_SIZE {
            return Err(ParseError::MessageTooLarge(data.len()));
        }

        let text = String::from_utf8_lossy(data);
        let (header_section, body) = split_header_body(&text);

        let mut lines = header_section.lines();
        let status_line = lines.next().ok_or(ParseError::Empty)?;

        // Parse: "RTSP/1.0 200 OK"
        let parts: Vec<&str> = status_line.splitn(3, ' ').collect();
        if parts.len() < 2 {
            return Err(ParseError::InvalidStatusLine(status_line.to_string()));
        }

        if parts[0] != "RTSP/1.0" {
            return Err(ParseError::UnsupportedVersion(parts[0].to_string()));
        }

        let status_code: u16 = parts[1]
            .parse()
            .map_err(|_| ParseError::InvalidStatusCode(parts[1].to_string()))?;

        let reason = if parts.len() > 2 {
            parts[2].to_string()
        } else {
            status_reason(status_code).to_string()
        };

        let headers = parse_headers(lines);

        Ok(Self {
            status_code,
            reason,
            headers,
            body: body.to_string(),
        })
    }
}

/// The result of trying to parse a message from a byte buffer.
pub enum ParsedMessage {
    Request(RtspRequest),
    Response(RtspResponse),
}

/// Try to parse a complete RTSP message from a buffer.
///
/// Returns `Ok(Some((message, consumed)))` if a complete message was parsed,
/// `Ok(None)` if the buffer doesn't contain a complete message yet,
/// or `Err` if the data is invalid.
pub fn try_parse_message(buf: &[u8]) -> Result<Option<(ParsedMessage, usize)>, ParseError> {
    if buf.len() > MAX_MESSAGE_SIZE {
        return Err(ParseError::MessageTooLarge(buf.len()));
    }

    // Find the header/body separator
    let separator_pos = find_header_end(buf);
    let (header_end, sep_len) = match separator_pos {
        Some(v) => v,
        None => return Ok(None), // Incomplete — need more data
    };

    // Parse Content-Length from headers
    let header_text = String::from_utf8_lossy(&buf[..header_end]);
    let content_length = extract_content_length(&header_text);

    if content_length > MAX_CONTENT_LENGTH {
        return Err(ParseError::ContentLengthExceeded(content_length));
    }

    // Check if we have the full message
    let total_length = header_end + sep_len + content_length;
    if buf.len() < total_length {
        return Ok(None); // Incomplete — need more data
    }

    // Extract the complete message
    let message_bytes = &buf[..total_length];

    // Determine if request or response
    let text = String::from_utf8_lossy(message_bytes);
    let trimmed = text.trim_start();

    let parsed = if trimmed.starts_with("RTSP/") {
        ParsedMessage::Response(RtspResponse::parse(message_bytes)?)
    } else {
        ParsedMessage::Request(RtspRequest::parse(message_bytes)?)
    };

    Ok(Some((parsed, total_length)))
}

// ─── Helpers ───────────────────────────────────────────────────────────────

fn split_header_body(text: &str) -> (&str, &str) {
    if let Some(pos) = text.find("\r\n\r\n") {
        (&text[..pos], text[pos + 4..].trim())
    } else if let Some(pos) = text.find("\n\n") {
        (&text[..pos], text[pos + 2..].trim())
    } else {
        (text, "")
    }
}

fn parse_headers<'a>(lines: impl Iterator<Item = &'a str>) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    headers
}

fn find_header_end(buf: &[u8]) -> Option<(usize, usize)> {
    buf.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|pos| (pos, 4))
        .or_else(|| {
            buf.windows(2)
                .position(|w| w == b"\n\n")
                .map(|pos| (pos, 2))
        })
}

fn extract_content_length(header_text: &str) -> usize {
    for line in header_text.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("content-length:") {
            if let Some(val) = line.split_once(':').map(|(_, v)| v.trim()) {
                return val.parse().unwrap_or(0);
            }
        }
    }
    0
}

fn status_reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        454 => "Session Not Found",
        455 => "Method Not Valid in This State",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_options_request() {
        let raw = b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nRequire: org.wfa.wfd1.0\r\n\r\n";
        let req = RtspRequest::parse(raw).unwrap();
        assert_eq!(req.method, RtspMethod::Options);
        assert_eq!(req.uri, "*");
        assert_eq!(req.cseq(), Some(1));
        assert_eq!(req.headers.get("Require").unwrap(), "org.wfa.wfd1.0");
        assert!(req.body.is_empty());
    }

    #[test]
    fn test_parse_request_with_body() {
        let raw = b"SET_PARAMETER rtsp://localhost/wfd1.0 RTSP/1.0\r\n\
                    CSeq: 5\r\n\
                    Content-Length: 25\r\n\r\n\
                    wfd_trigger_method: SETUP";
        let req = RtspRequest::parse(raw).unwrap();
        assert_eq!(req.method, RtspMethod::SetParameter);
        assert!(req.body.contains("wfd_trigger_method: SETUP"));
    }

    #[test]
    fn test_parse_200_ok_response() {
        let raw = b"RTSP/1.0 200 OK\r\n\
                    CSeq: 1\r\n\
                    Public: GET_PARAMETER, SET_PARAMETER\r\n\r\n";
        let resp = RtspResponse::parse(raw).unwrap();
        assert_eq!(resp.status_code, 200);
        assert_eq!(resp.reason, "OK");
        assert_eq!(resp.cseq(), Some(1));
    }

    #[test]
    fn test_serialize_request_roundtrip() {
        let req = RtspRequest::new(RtspMethod::Options, "*")
            .with_cseq(1)
            .with_header("Require", "org.wfa.wfd1.0");
        let data = req.serialize();
        let parsed = RtspRequest::parse(&data).unwrap();
        assert_eq!(parsed.method, RtspMethod::Options);
        assert_eq!(parsed.cseq(), Some(1));
    }

    #[test]
    fn test_serialize_response_roundtrip() {
        let resp = RtspResponse::ok(3).with_body("wfd_video_formats: 00 00 01 02");
        let data = resp.serialize();
        let parsed = RtspResponse::parse(&data).unwrap();
        assert_eq!(parsed.status_code, 200);
        assert_eq!(parsed.cseq(), Some(3));
        assert!(parsed.body.contains("wfd_video_formats"));
    }

    #[test]
    fn test_unknown_method_error() {
        let raw = b"INVALID * RTSP/1.0\r\nCSeq: 1\r\n\r\n";
        assert!(matches!(
            RtspRequest::parse(raw),
            Err(ParseError::UnknownMethod(_))
        ));
    }

    #[test]
    fn test_invalid_version_error() {
        let raw = b"OPTIONS * RTSP/2.0\r\nCSeq: 1\r\n\r\n";
        assert!(matches!(
            RtspRequest::parse(raw),
            Err(ParseError::UnsupportedVersion(_))
        ));
    }

    #[test]
    fn test_try_parse_incomplete() {
        let buf = b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n";
        assert!(try_parse_message(buf).unwrap().is_none());
    }

    #[test]
    fn test_try_parse_complete_request() {
        let buf = b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n";
        let result = try_parse_message(buf).unwrap().unwrap();
        assert_eq!(result.1, buf.len());
        assert!(matches!(result.0, ParsedMessage::Request(_)));
    }

    #[test]
    fn test_try_parse_complete_response() {
        let buf = b"RTSP/1.0 200 OK\r\nCSeq: 1\r\n\r\n";
        let result = try_parse_message(buf).unwrap().unwrap();
        assert!(matches!(result.0, ParsedMessage::Response(_)));
    }

    #[test]
    fn test_session_id_with_timeout() {
        let raw =
            b"PLAY rtsp://x/wfd1.0 RTSP/1.0\r\nCSeq: 1\r\nSession: deadbeef;timeout=30\r\n\r\n";
        let req = RtspRequest::parse(raw).unwrap();
        assert_eq!(req.session_id(), Some("deadbeef"));
    }
}
