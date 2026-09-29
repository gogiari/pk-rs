use crate::socks::connect_socks5;
use crate::{log_error, log_info};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub async fn run_http_proxy(listen_port: u16, socks_port: u16) -> Result<(), String> {
    let listen_addr: SocketAddr = format!("127.0.0.1:{}", listen_port)
        .parse()
        .map_err(|e| format!("{}", e))?;
    let socks_addr: SocketAddr = format!("127.0.0.1:{}", socks_port)
        .parse()
        .map_err(|e| format!("{}", e))?;

    let listener = TcpListener::bind(listen_addr)
        .await
        .map_err(|e| format!("Cannot bind HTTP proxy port {}: {}", listen_port, e))?;

    log_info!("[HTTP Proxy] Listening on http://{}", listen_addr);

    loop {
        match listener.accept().await {
            Ok((client, peer_addr)) => {
                tokio::spawn(async move {
                    if let Err(e) = handle_client(client, socks_addr, peer_addr).await {
                        // Suppress noisy client reset / abort errors
                        if !e.contains("Broken pipe") && !e.contains("Connection reset") {
                            log_error!("[HTTP Proxy] Client ({}) error: {}", peer_addr, e);
                        }
                    }
                });
            }
            Err(e) => {
                log_error!("[HTTP Proxy] Accept error: {}", e);
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

async fn handle_client(
    mut client: TcpStream,
    socks_addr: SocketAddr,
    _peer_addr: SocketAddr,
) -> Result<(), String> {
    let _ = client.set_nodelay(true);

    // Read the HTTP request header up to \r\n\r\n
    let mut buf = Vec::with_capacity(2048);
    let mut temp = [0u8; 1024];

    loop {
        let n = client
            .read(&mut temp)
            .await
            .map_err(|e| format!("Read header error: {}", e))?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&temp[..n]);

        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            let header_bytes = &buf[..pos];
            let header_str = String::from_utf8_lossy(header_bytes);
            let first_line = header_str.lines().next().unwrap_or("");
            let parts: Vec<&str> = first_line.split_whitespace().collect();

            if parts.len() < 3 {
                let resp = b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n";
                let _ = client.write_all(resp).await;
                return Err(format!("Malformed HTTP request line: '{}'", first_line));
            }

            let method = parts[0];
            let target = parts[1];
            let version = parts[2];

            if method.eq_ignore_ascii_case("CONNECT") {
                let (host, port) = parse_host_port(target, 443)?;

                let mut upstream = match connect_socks5(
                    socks_addr,
                    &host,
                    port,
                    Duration::from_secs(30),
                )
                .await {
                    Ok(s) => s,
                    Err(e) => {
                        let resp = format!(
                            "HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                            e.len(),
                            e
                        );
                        let _ = client.write_all(resp.as_bytes()).await;
                        return Err(format!("Upstream SOCKS5 CONNECT failed for {}:{}: {}", host, port, e));
                    }
                };

                let _ = upstream.set_nodelay(true);

                // Reply HTTP 200 Connection Established
                client
                    .write_all(b"HTTP/1.1 200 Connection Established\r\nProxy-Agent: pk-rust\r\n\r\n")
                    .await
                    .map_err(|e| format!("Send 200 error: {}", e))?;

                // Any leftover bytes after \r\n\r\n
                let leftover = &buf[pos + 4..];
                if !leftover.is_empty() {
                    upstream
                        .write_all(leftover)
                        .await
                        .map_err(|e| format!("Forward leftover error: {}", e))?;
                }

                // Bidirectional copy using copy_bidirectional for clean half-close & streaming
                let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
                return Ok(());
            } else {
                // Plain HTTP request (e.g. GET http://example.com/path HTTP/1.1)
                let (host, port, request_bytes) = parse_plain_http_request(method, target, version, header_bytes)?;

                let mut upstream = match connect_socks5(
                    socks_addr,
                    &host,
                    port,
                    Duration::from_secs(30),
                )
                .await {
                    Ok(s) => s,
                    Err(e) => {
                        let resp = format!(
                            "HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                            e.len(),
                            e
                        );
                        let _ = client.write_all(resp.as_bytes()).await;
                        return Err(format!("Upstream SOCKS5 plain HTTP failed for {}:{}: {}", host, port, e));
                    }
                };

                let _ = upstream.set_nodelay(true);

                upstream
                    .write_all(&request_bytes)
                    .await
                    .map_err(|e| format!("Forward plain HTTP request error: {}", e))?;

                let leftover = &buf[pos + 4..];
                if !leftover.is_empty() {
                    upstream
                        .write_all(leftover)
                        .await
                        .map_err(|e| format!("Forward leftover body error: {}", e))?;
                }

                let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
                return Ok(());
            }
        }

        if buf.len() > 65536 {
            let resp = b"HTTP/1.1 431 Request Header Fields Too Large\r\nContent-Length: 0\r\n\r\n";
            let _ = client.write_all(resp).await;
            return Err("HTTP header too large".to_string());
        }
    }
}

fn parse_plain_http_request(
    method: &str,
    target: &str,
    version: &str,
    header_bytes: &[u8],
) -> Result<(String, u16, Vec<u8>), String> {
    let (host, port, path) = if target.starts_with("http://") || target.starts_with("HTTP://") {
        let rest = &target[7..];
        let (host_port, path_part) = match rest.find('/') {
            Some(idx) => (&rest[..idx], &rest[idx..]),
            None => (rest, "/"),
        };
        let (h, p) = parse_host_port(host_port, 80)?;
        (h, p, path_part.to_string())
    } else {
        return Err("Plain HTTP request requires absolute URL (e.g. http://...)".to_string());
    };

    let mut reconstructed = Vec::new();
    reconstructed.extend_from_slice(format!("{} {} {}\r\n", method, path, version).as_bytes());

    let header_str = String::from_utf8_lossy(header_bytes);
    for line in header_str.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("proxy-connection:") || lower.starts_with("proxy-authorization:") {
            continue;
        }
        reconstructed.extend_from_slice(trimmed.as_bytes());
        reconstructed.extend_from_slice(b"\r\n");
    }
    reconstructed.extend_from_slice(b"\r\n");

    Ok((host, port, reconstructed))
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn parse_host_port(target: &str, default_port: u16) -> Result<(String, u16), String> {
    if let Some(pos) = target.rfind(':') {
        let host = &target[..pos];
        let port_str = &target[pos + 1..];
        let port = port_str.parse::<u16>().map_err(|_| "Invalid port")?;
        Ok((host.trim_matches('[').trim_matches(']').to_string(), port))
    } else {
        Ok((target.to_string(), default_port))
    }
}
