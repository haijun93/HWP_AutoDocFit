//! Everything HTTP server adapter and HTTP/1.1 parsing utilities.
//! No external HTTP crates; uses standard library `std::net::TcpStream`.

use crate::{sha256, AdapterResult, Query, QueryAdapter};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

/// RFC 3986 percent-encoding for query string values.
pub fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for &b in input.as_bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(out, "%{:02X}", b);
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
    pub status_code: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Parses an HTTP/1.1 response from a reader.
pub fn parse_http_response<R: Read>(reader: &mut R) -> Result<HttpResponse, String> {
    let mut buf_reader = BufReader::new(reader);

    // 1. 상태줄 파싱
    let mut status_line = String::new();
    buf_reader
        .read_line(&mut status_line)
        .map_err(|e| format!("failed to read HTTP status line: {e}"))?;

    let status_line_trimmed = status_line.trim_end();
    if status_line_trimmed.is_empty() {
        return Err("empty HTTP response".into());
    }

    let mut parts = status_line_trimmed.splitn(3, ' ');
    let _version = parts
        .next()
        .ok_or_else(|| "missing HTTP version".to_string())?;
    let code_str = parts
        .next()
        .ok_or_else(|| "missing HTTP status code".to_string())?;
    let status_code: u16 = code_str
        .parse()
        .map_err(|e| format!("invalid HTTP status code '{code_str}': {e}"))?;
    let status_text = parts.next().unwrap_or("").to_string();

    // 2. 헤더 파싱
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        buf_reader
            .read_line(&mut line)
            .map_err(|e| format!("failed to read HTTP header: {e}"))?;
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            // 헤더 종료
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }

    // 3. 본문 파싱 (chunked vs Content-Length vs EOF)
    let is_chunked = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.to_lowercase().contains("chunked"));

    let content_length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok());

    let mut body = Vec::new();

    if is_chunked {
        loop {
            let mut size_line = String::new();
            buf_reader
                .read_line(&mut size_line)
                .map_err(|e| format!("failed to read chunk size: {e}"))?;
            let size_str = size_line.trim_end();
            if size_str.is_empty() {
                continue;
            }
            // 청크 확장이 있을 수 있으므로 ';' 분리
            let hex_str = size_str.split(';').next().unwrap_or("").trim();
            let chunk_size = usize::from_str_radix(hex_str, 16)
                .map_err(|e| format!("invalid chunk size hex '{hex_str}': {e}"))?;

            if chunk_size == 0 {
                // 트레일러 헤더 및 빈 줄 소비
                loop {
                    let mut trailer = String::new();
                    buf_reader
                        .read_line(&mut trailer)
                        .map_err(|e| format!("failed to read chunk trailer: {e}"))?;
                    if trailer.trim_end().is_empty() {
                        break;
                    }
                }
                break;
            }

            let mut chunk_buf = vec![0u8; chunk_size];
            buf_reader
                .read_exact(&mut chunk_buf)
                .map_err(|e| format!("failed to read chunk body: {e}"))?;
            body.extend_from_slice(&chunk_buf);

            // 청크 뒤의 CRLF 소비
            let mut crlf = [0u8; 2];
            buf_reader
                .read_exact(&mut crlf)
                .map_err(|e| format!("failed to read chunk CRLF: {e}"))?;
        }
    } else if let Some(len) = content_length {
        body.resize(len, 0);
        buf_reader
            .read_exact(&mut body)
            .map_err(|e| format!("failed to read HTTP body with content-length {len}: {e}"))?;
    } else {
        // Connection: close 등 길이 지정 없는 경우 EOF까지
        buf_reader
            .read_to_end(&mut body)
            .map_err(|e| format!("failed to read HTTP body to EOF: {e}"))?;
    }

    Ok(HttpResponse {
        status_code,
        status_text,
        headers,
        body,
    })
}

/// Everything HTTP 서버 어댑터
#[derive(Debug, Clone)]
pub struct EverythingHttpAdapter {
    pub name: String,
    pub host: String,
    pub port: u16,
}

impl EverythingHttpAdapter {
    pub fn new(port: u16) -> Self {
        Self {
            name: "everything_http".into(),
            host: "127.0.0.1".into(),
            port,
        }
    }

    pub fn with_host(host: impl Into<String>, port: u16) -> Self {
        Self {
            name: "everything_http".into(),
            host: host.into(),
            port,
        }
    }
}

impl QueryAdapter for EverythingHttpAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn query(&mut self, query: &Query, top_n: usize, time_limit: Duration) -> AdapterResult {
        let started = Instant::now();
        let addr_str = format!("{}:{}", self.host, self.port);
        let sock_addr: SocketAddr = match addr_str.parse() {
            Ok(a) => a,
            Err(e) => {
                return AdapterResult {
                    latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                    result_count: 0,
                    top_ids: vec![],
                    error: Some(format!("invalid socket address {addr_str}: {e}")),
                    timed_out: false,
                };
            }
        };

        let mut stream = match TcpStream::connect_timeout(&sock_addr, time_limit) {
            Ok(s) => s,
            Err(e) => {
                let timed_out = e.kind() == std::io::ErrorKind::TimedOut;
                return AdapterResult {
                    latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                    result_count: 0,
                    top_ids: vec![],
                    error: Some(format!("HTTP connection failed to {addr_str}: {e}")),
                    timed_out,
                };
            }
        };

        if let Err(e) = stream.set_read_timeout(Some(time_limit)) {
            return AdapterResult {
                latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                result_count: 0,
                top_ids: vec![],
                error: Some(format!("set_read_timeout failed: {e}")),
                timed_out: false,
            };
        }
        if let Err(e) = stream.set_write_timeout(Some(time_limit)) {
            return AdapterResult {
                latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                result_count: 0,
                top_ids: vec![],
                error: Some(format!("set_write_timeout failed: {e}")),
                timed_out: false,
            };
        }

        let encoded_query = url_encode(&query.text);
        let req = format!(
            "GET /?s={encoded_query}&j=1&count={top_n}&path_column=1 HTTP/1.1\r\n\
             Host: {}:{}\r\n\
             Connection: close\r\n\
             User-Agent: qms_bench\r\n\
             \r\n",
            self.host, self.port
        );

        if let Err(e) = stream.write_all(req.as_bytes()) {
            let timed_out = e.kind() == std::io::ErrorKind::TimedOut;
            return AdapterResult {
                latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                result_count: 0,
                top_ids: vec![],
                error: Some(format!("HTTP request write failed: {e}")),
                timed_out,
            };
        }

        let resp = match parse_http_response(&mut stream) {
            Ok(r) => r,
            Err(e) => {
                return AdapterResult {
                    latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                    result_count: 0,
                    top_ids: vec![],
                    error: Some(format!("HTTP response parse failed: {e}")),
                    timed_out: false,
                };
            }
        };

        let latency_ms = started.elapsed().as_secs_f64() * 1000.0;

        // 401 Unauthorized 처리 (인증 필요 상태)
        if resp.status_code == 401 {
            return AdapterResult {
                latency_ms,
                result_count: 0,
                top_ids: vec![],
                error: Some("인증 필요 (HTTP 401 Unauthorized)".into()),
                timed_out: false,
            };
        }

        if resp.status_code != 200 {
            return AdapterResult {
                latency_ms,
                result_count: 0,
                top_ids: vec![],
                error: Some(format!(
                    "HTTP server returned status {} {}",
                    resp.status_code, resp.status_text
                )),
                timed_out: false,
            };
        }

        // 200 OK JSON 파싱
        match parse_everything_json_response(&resp.body, top_n) {
            Ok((total, top_ids)) => AdapterResult {
                latency_ms,
                result_count: total,
                top_ids,
                error: None,
                timed_out: false,
            },
            Err(e) => AdapterResult {
                latency_ms,
                result_count: 0,
                top_ids: vec![],
                error: Some(format!("Everything JSON parse error: {e}")),
                timed_out: false,
            },
        }
    }
}

/// Everything JSON 응답(`{totalResults: N, results: [...]}`)을 파싱하여 (총 건수, 상위 N개 SHA256 해시)를 반환.
pub fn parse_everything_json_response(
    body: &[u8],
    top_n: usize,
) -> Result<(u64, Vec<String>), String> {
    let v: Value =
        serde_json::from_slice(body).map_err(|e| format!("invalid JSON in HTTP response: {e}"))?;

    let total = v["totalResults"].as_u64().unwrap_or(0);
    let results_array = v["results"]
        .as_array()
        .ok_or_else(|| "missing results array in Everything JSON response".to_string())?;

    let top_ids = results_array
        .iter()
        .take(top_n)
        .map(|item| {
            let name = item["name"].as_str().unwrap_or_default();
            let path = item["path"].as_str().unwrap_or_default();
            let full_path = if path.is_empty() {
                name.to_string()
            } else if path.ends_with('\\') || path.ends_with('/') {
                format!("{path}{name}")
            } else {
                format!("{path}\\{name}")
            };
            sha256(full_path.as_bytes())
        })
        .collect();

    Ok((total, top_ids))
}

#[derive(Debug, Clone, PartialEq)]
pub enum PortDetectResult {
    /// Everything HTTP 서버 감지됨 (사용 가능)
    Available(u16),
    /// Everything HTTP 서버 감지되었으나 인증 필요 (401)
    AuthRequired(u16),
    /// 감지되지 않음
    NotFound,
}

/// 127.0.0.1의 지정 포트(기본 80, 8080, 8888)에 짧은 시간 제한으로 연결 시도하고 Everything HTTP 응답 머리말 확인.
/// 읽기 전용 GET 1회만 수행.
pub fn detect_everything_http_port(
    host: &str,
    ports: &[u16],
    timeout: Duration,
) -> PortDetectResult {
    for &port in ports {
        let addr_str = format!("{host}:{port}");
        let sock_addr: SocketAddr = match addr_str.parse() {
            Ok(a) => a,
            Err(_) => continue,
        };

        let mut stream = match TcpStream::connect_timeout(&sock_addr, timeout) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));

        // 읽기 전용 GET 1회 요청
        let req = format!(
            "GET /?j=1&count=0 HTTP/1.1\r\n\
             Host: {host}:{port}\r\n\
             Connection: close\r\n\
             User-Agent: qms_bench\r\n\
             \r\n"
        );

        if stream.write_all(req.as_bytes()).is_err() {
            continue;
        }

        if let Ok(resp) = parse_http_response(&mut stream) {
            let is_everything_server = resp
                .header("server")
                .map(|s| s.to_lowercase().contains("everything"))
                .unwrap_or(false);

            if resp.status_code == 401 {
                let is_everything_auth = resp
                    .header("www-authenticate")
                    .map(|s| s.to_lowercase().contains("everything"))
                    .unwrap_or(false);
                if is_everything_server || is_everything_auth {
                    return PortDetectResult::AuthRequired(port);
                }
            } else if resp.status_code == 200 {
                // 200 OK인 경우, Server 헤더 또는 JSON body에 totalResults 필드 확인
                let has_total_results =
                    serde_json::from_slice::<Value>(&resp.body)
                        .map(|v| v.get("totalResults").is_some())
                        .unwrap_or(false);
                if is_everything_server || has_total_results {
                    return PortDetectResult::Available(port);
                }
            }
        }
    }

    PortDetectResult::NotFound
}
