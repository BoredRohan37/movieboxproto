//! Minimal HTTP/1.1 server primitives for the MovieBox GUI server.
//!
//! Hand-rolled on top of tokio TCP to keep the dependency tree (and binary)
//! tiny — critical for low-spec Windows 7/8.1 hardware. Supports what the GUI
//! needs: request parsing, Range requests (206), keep-alive, streaming bodies.

use std::collections::HashMap;

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

pub const MAX_REQUEST_LINE: usize = 8 * 1024;
pub const MAX_HEADERS: usize = 100;
pub const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    /// Original request target (path + query), undecoded — used by the
    /// playback proxy to preserve signed URLs byte-for-byte.
    pub raw_target: String,
    /// Lower-cased header names → value.
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub keep_alive: bool,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(|s| s.as_str())
    }

    pub fn query_param(&self, name: &str) -> Option<&str> {
        self.query.get(name).map(|s| s.as_str())
    }
}

#[derive(Debug)]
pub enum ParseError {
    Eof,
    TooLarge,
    Malformed(&'static str),
    Io(std::io::Error),
}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        ParseError::Io(e)
    }
}

pub fn urldecode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(h), Some(l)) = (hi, lo) {
                    out.push(((h << 4) | l) as u8);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        if !k.is_empty() {
            map.insert(urldecode(k), urldecode(v));
        }
    }
    map
}

pub async fn read_request(reader: &mut BufReader<OwnedReadHalf>) -> Result<Request, ParseError> {
    let mut line = String::new();
    let n = read_capped_line(reader, &mut line, MAX_REQUEST_LINE).await?;
    if n == 0 {
        return Err(ParseError::Eof);
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_ascii_uppercase();
    let target = parts.next().unwrap_or("");
    if method.is_empty() || target.is_empty() {
        return Err(ParseError::Malformed("bad request line"));
    }
    let (path_raw, query_raw) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    };
    let path = urldecode(path_raw);
    let query = parse_query(query_raw);

    let mut headers = HashMap::new();
    let mut header_line = String::new();
    for _ in 0..MAX_HEADERS {
        header_line.clear();
        let n = read_capped_line(reader, &mut header_line, MAX_REQUEST_LINE).await?;
        if n == 0 {
            break;
        }
        let trimmed = header_line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let mut body = Vec::new();
    if let Some(cl) = headers.get("content-length") {
        let len: usize = cl
            .parse()
            .map_err(|_| ParseError::Malformed("bad content-length"))?;
        if len > MAX_BODY_BYTES {
            return Err(ParseError::TooLarge);
        }
        body.resize(len, 0);
        reader.read_exact(&mut body).await?;
    }

    let keep_alive = match headers.get("connection").map(|s| s.to_ascii_lowercase()) {
        Some(c) if c.contains("close") => false,
        _ => true,
    };

    Ok(Request {
        method,
        path,
        query,
        raw_target: target.to_string(),
        headers,
        body,
        keep_alive,
    })
}

async fn read_capped_line(
    reader: &mut BufReader<OwnedReadHalf>,
    out: &mut String,
    cap: usize,
) -> Result<usize, ParseError> {
    let mut buf = Vec::with_capacity(128);
    let mut byte = [0u8; 1];
    loop {
        let n = reader.read(&mut byte).await?;
        if n == 0 {
            break;
        }
        buf.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
        if buf.len() > cap {
            return Err(ParseError::TooLarge);
        }
    }
    let len = buf.len();
    out.push_str(&String::from_utf8_lossy(&buf));
    Ok(len)
}

pub struct StreamSource {
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<(String, String)>,
    pub chunks: tokio::sync::mpsc::Receiver<Result<Vec<u8>, String>>,
}

pub struct Response {
    pub status: u16,
    pub reason: &'static str,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// When set, body is streamed from this source instead of `body`.
    pub stream: Option<StreamSource>,
}

impl Response {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            reason: reason_for(status),
            headers: Vec::new(),
            body: Vec::new(),
            stream: None,
        }
    }

    pub fn json(status: u16, value: &serde_json::Value) -> Self {
        let mut r = Self::new(status);
        r.headers.push((
            "Content-Type".into(),
            "application/json; charset=utf-8".into(),
        ));
        r.body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
        r
    }

    pub fn json_error(status: u16, message: &str) -> Self {
        Self::json(status, &serde_json::json!({ "error": message }))
    }

    pub fn html(status: u16, body: impl Into<Vec<u8>>) -> Self {
        let mut r = Self::new(status);
        r.headers
            .push(("Content-Type".into(), "text/html; charset=utf-8".into()));
        r.body = body.into();
        r
    }

    pub fn bytes(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        let mut r = Self::new(status);
        r.headers.push(("Content-Type".into(), content_type.into()));
        r.body = body;
        r
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn no_store(mut self) -> Self {
        self.headers
            .push(("Cache-Control".into(), "no-store".into()));
        self
    }
}

pub fn reason_for(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        416 => "Range Not Satisfiable",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        504 => "Gateway Timeout",
        _ => "OK",
    }
}

/// Parse a Range header value like `bytes=0-1023`, `bytes=500-`, `bytes=-500`.
/// Returns (start, end_inclusive) or None when unsatisfiable.
pub fn parse_range(header: &str, total: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?;
    let first = spec.split(',').next()?.trim();
    let (start_s, end_s) = first.split_once('-')?;
    if start_s.is_empty() {
        let n: u64 = end_s.trim().parse().ok()?;
        if n == 0 || total == 0 {
            return None;
        }
        let start = total.saturating_sub(n);
        return Some((start, total.saturating_sub(1)));
    }
    let start: u64 = start_s.trim().parse().ok()?;
    let end = if end_s.trim().is_empty() {
        total.saturating_sub(1)
    } else {
        end_s
            .trim()
            .parse::<u64>()
            .ok()?
            .min(total.saturating_sub(1))
    };
    if start > end || total == 0 || start >= total {
        return None;
    }
    Some((start, end))
}

pub async fn write_response_head(
    writer: &mut OwnedWriteHalf,
    status: u16,
    reason: &str,
    headers: &[(String, String)],
    content_length: Option<u64>,
) -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {} {}\r\n", status, reason);
    for (name, value) in headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    if let Some(len) = content_length {
        head.push_str(&format!("Content-Length: {}\r\n", len));
    }
    head.push_str("Server: MovieBox\r\n\r\n");
    writer.write_all(head.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn write_simple_response(
    writer: &mut OwnedWriteHalf,
    resp: &Response,
    head_only: bool,
) -> std::io::Result<()> {
    let mut headers = resp.headers.clone();
    headers.push(("Server".into(), "MovieBox".into()));
    let len = Some(resp.body.len() as u64);
    write_response_head(writer, resp.status, resp.reason, &headers, len).await?;
    if !head_only {
        writer.write_all(&resp.body).await?;
        writer.flush().await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urldecode_basic() {
        assert_eq!(urldecode("hello%20world"), "hello world");
        assert_eq!(urldecode("a+b"), "a b");
        assert_eq!(urldecode("%E4%BD%A0"), "你");
        assert_eq!(urldecode("100%"), "100%");
    }

    #[test]
    fn range_parsing() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=500-", 1000), Some((500, 999)));
        assert_eq!(parse_range("bytes=-200", 1000), Some((800, 999)));
        assert_eq!(parse_range("bytes=999-", 1000), Some((999, 999)));
        assert_eq!(parse_range("bytes=100-50", 1000), None);
        assert_eq!(parse_range("bytes=0-", 0), None);
        assert_eq!(parse_range("bytes=1000-", 1000), None);
    }
}
