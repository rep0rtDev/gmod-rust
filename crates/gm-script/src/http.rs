//! A minimal HTTP/1.1 GET over `std::net` for registries on plain static hosts: integrity comes
//! from the sha256 and the signatures of what is fetched, so no TLS is needed for correctness
//! (an https registry goes through a local mirror or a proxy for now). Handles `Content-Length`
//! and chunked bodies, no redirects, 30 s timeout, 64 MiB cap.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub const MAX_BODY: usize = 64 << 20;

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("bad url `{0}`")]
    Url(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("http {0} for {1}")]
    Status(u16, String),
    #[error("malformed response for {0}")]
    Malformed(String),
    #[error("body of {0} exceeds {max} bytes", max = MAX_BODY)]
    TooLarge(String),
}

/// `http://host[:port]/path` -> (host, port, path)
pub fn split_url(url: &str) -> Result<(String, u16, String), HttpError> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| HttpError::Url(url.to_string()))?;
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if !h.contains(']') || h.ends_with(']') => (
            h.to_string(),
            p.parse().map_err(|_| HttpError::Url(url.to_string()))?,
        ),
        _ => (hostport.to_string(), 80),
    };
    if host.is_empty() {
        return Err(HttpError::Url(url.to_string()));
    }
    Ok((host, port, path.to_string()))
}

pub fn get(url: &str) -> Result<Vec<u8>, HttpError> {
    let (status, body, _) = request("GET", url, None, &[])?;
    if status != 200 {
        return Err(HttpError::Status(status, url.to_string()));
    }
    Ok(body)
}

/// One request with any method, body and extra headers: status, body and response headers
/// (lower-case names); only the transport and a malformed reply are errors.
/// `(status, body, headers)` of a finished request.
pub type HttpResponse = (u16, Vec<u8>, Vec<(String, String)>);

pub fn request(
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    headers: &[(String, String)],
) -> Result<HttpResponse, HttpError> {
    let (host, port, path) = split_url(url)?;
    let addr = (host.as_str(), port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| HttpError::Url(url.to_string()))?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(10))?;
    s.set_read_timeout(Some(Duration::from_secs(30)))?;
    s.set_write_timeout(Some(Duration::from_secs(30)))?;
    let method = method.to_ascii_uppercase();
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: gmr\r\nConnection: close\r\n"
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if let Some(b) = body {
        req.push_str(&format!("Content-Length: {}\r\n", b.len()));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes())?;
    if let Some(b) = body {
        s.write_all(b)?;
    }
    let mut raw = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    let mut header_end = None;
    loop {
        let n = s.read(&mut buf)?;
        if n == 0 {
            break;
        }
        raw.extend_from_slice(&buf[..n]);
        if header_end.is_none() {
            header_end = raw.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4);
        }
        if raw.len() > MAX_BODY + 64 * 1024 {
            return Err(HttpError::TooLarge(url.to_string()));
        }
    }
    let Some(he) = header_end else {
        return Err(HttpError::Malformed(url.to_string()));
    };
    let head = String::from_utf8_lossy(&raw[..he]).to_string();
    let mut lines = head.split("\r\n");
    let status: u16 = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| HttpError::Malformed(url.to_string()))?;
    let mut length = None;
    let mut chunked = false;
    let mut resp_headers = Vec::new();
    for l in lines {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        match key.as_str() {
            "content-length" => length = v.trim().parse::<usize>().ok(),
            "transfer-encoding" => chunked = v.to_ascii_lowercase().contains("chunked"),
            _ => {}
        }
        resp_headers.push((key, v.trim().to_string()));
    }
    let body = &raw[he..];
    let out = if chunked {
        let mut out = Vec::new();
        let mut i = 0;
        loop {
            let Some(nl) = body[i..].windows(2).position(|w| w == b"\r\n") else {
                return Err(HttpError::Malformed(url.to_string()));
            };
            let size_str = String::from_utf8_lossy(&body[i..i + nl]).to_string();
            let size = usize::from_str_radix(size_str.split(';').next().unwrap_or("").trim(), 16)
                .map_err(|_| HttpError::Malformed(url.to_string()))?;
            i += nl + 2;
            if size == 0 {
                break;
            }
            if i + size > body.len() {
                return Err(HttpError::Malformed(url.to_string()));
            }
            out.extend_from_slice(&body[i..i + size]);
            i += size + 2;
        }
        out
    } else {
        match length {
            Some(n) if n <= body.len() => body[..n].to_vec(),
            Some(_) => return Err(HttpError::Malformed(url.to_string())),
            None => body.to_vec(),
        }
    };
    if out.len() > MAX_BODY {
        return Err(HttpError::TooLarge(url.to_string()));
    }
    Ok((status, out, resp_headers))
}

/// Serves a directory over plain HTTP on a loopback port for tests: returns the base url.
#[doc(hidden)]
/// A registry over plain http on `listener`, forever: `GET /index.toml` and
/// `GET /blobs/<sha>.gmr` read the directory, `POST /publish` (body: a signed `.gmr`) runs
/// `Registry::publish` and answers `200 <id> <version> <sha>` or the error's text with 400
/// (unsigned / malformed), 409 (version already published), 413 (too large).
pub fn serve(dir: std::path::PathBuf, listener: std::net::TcpListener) {
    for stream in listener.incoming() {
        let Ok(mut s) = stream else { continue };
        let _ = s.set_read_timeout(Some(Duration::from_secs(30)));
        let mut req = Vec::new();
        let mut b = [0u8; 4096];
        while !req.windows(4).any(|w| w == b"\r\n\r\n") {
            match s.read(&mut b) {
                Ok(0) | Err(_) => break,
                Ok(n) => req.extend_from_slice(&b[..n]),
            }
        }
        let Some(head_end) = req.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&req[..head_end]).to_string();
        let mut lines = head.lines();
        let line = lines.next().unwrap_or_default().to_string();
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or("GET").to_ascii_uppercase();
        let path = parts.next().unwrap_or("/").to_string();
        let length: usize = lines
            .filter_map(|l| l.split_once(':'))
            .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, v)| v.trim().parse().ok())
            .unwrap_or(0);
        let mut body = req[head_end + 4..].to_vec();
        while body.len() < length {
            match s.read(&mut b) {
                Ok(0) | Err(_) => break,
                Ok(n) => body.extend_from_slice(&b[..n]),
            }
        }
        let (status, text): (u16, Vec<u8>) = match (method.as_str(), path.as_str()) {
            ("GET", "/index.toml") => match std::fs::read(dir.join("index.toml")) {
                Ok(bytes) => (200, bytes),
                Err(_) => (404, Vec::new()),
            },
            ("GET", p) if p.starts_with("/blobs/") && p.ends_with(".gmr") => {
                let name = &p["/blobs/".len()..];
                if name.contains('/') || name.contains("..") {
                    (404, Vec::new())
                } else {
                    match std::fs::read(dir.join("blobs").join(name)) {
                        Ok(bytes) => (200, bytes),
                        Err(_) => (404, Vec::new()),
                    }
                }
            }
            ("POST", "/publish") => {
                match crate::Registry::open(&dir).and_then(|mut r| r.publish(&body)) {
                    Ok(e) => (
                        200,
                        format!("{} {} {}", e.id, e.version, e.sha).into_bytes(),
                    ),
                    Err(e) => {
                        let status = match e {
                            crate::registry::RegistryError::Conflict(..) => 409,
                            crate::registry::RegistryError::TooLarge(..) => 413,
                            _ => 400,
                        };
                        (status, e.to_string().into_bytes())
                    }
                }
            }
            _ => (404, Vec::new()),
        };
        let reason = match status {
            200 => "OK",
            400 => "Bad Request",
            409 => "Conflict",
            413 => "Payload Too Large",
            _ => "Not Found",
        };
        let mut r = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            text.len()
        )
        .into_bytes();
        r.extend_from_slice(&text);
        let _ = s.write_all(&r);
    }
}

/// `serve` on an ephemeral loopback port in a thread; returns the base url.
pub fn serve_for_tests(dir: std::path::PathBuf) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || serve(dir, listener));
    format!("http://{addr}")
}

pub fn serve_dir_for_tests(dir: std::path::PathBuf, chunked: bool) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { break };
            let mut req = Vec::new();
            let mut b = [0u8; 4096];
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut b) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => req.extend_from_slice(&b[..n]),
                }
            }
            let line = String::from_utf8_lossy(&req)
                .lines()
                .next()
                .unwrap_or_default()
                .to_string();
            let path = line.split_whitespace().nth(1).unwrap_or("/");
            let file = dir.join(path.trim_start_matches('/'));
            let resp = match std::fs::read(&file) {
                Ok(bytes) if chunked => {
                    let mut r = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
                    for c in bytes.chunks(7000) {
                        r.extend_from_slice(format!("{:x}\r\n", c.len()).as_bytes());
                        r.extend_from_slice(c);
                        r.extend_from_slice(b"\r\n");
                    }
                    r.extend_from_slice(b"0\r\n\r\n");
                    r
                }
                Ok(bytes) => {
                    let mut r =
                        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", bytes.len())
                            .into_bytes();
                    r.extend_from_slice(&bytes);
                    r
                }
                Err(_) => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
            };
            let _ = s.write_all(&resp);
        }
    });
    format!("http://{addr}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_split_and_bodies_arrive_plain_or_chunked() {
        assert_eq!(
            split_url("http://example.org/reg/index.toml").unwrap(),
            ("example.org".into(), 80, "/reg/index.toml".into())
        );
        assert_eq!(
            split_url("http://10.0.0.1:8080").unwrap(),
            ("10.0.0.1".into(), 8080, "/".into())
        );
        assert!(split_url("https://x/").is_err());
        let dir = std::env::temp_dir().join(format!("gm-http-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("blobs")).unwrap();
        let big: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(dir.join("blobs/a.gmr"), &big).unwrap();
        std::fs::write(dir.join("index.toml"), "x = 1\n").unwrap();
        for chunked in [false, true] {
            let base = serve_dir_for_tests(dir.clone(), chunked);
            assert_eq!(get(&format!("{base}/index.toml")).unwrap(), b"x = 1\n");
            assert_eq!(get(&format!("{base}/blobs/a.gmr")).unwrap(), big);
            assert!(matches!(
                get(&format!("{base}/missing")),
                Err(HttpError::Status(404, _))
            ));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
