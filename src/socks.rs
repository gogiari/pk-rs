use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

pub async fn connect_socks5(
    socks_addr: SocketAddr,
    target_host: &str,
    target_port: u16,
    conn_timeout: Duration,
) -> Result<TcpStream, String> {
    let connect_future = TcpStream::connect(socks_addr);
    let mut stream = timeout(conn_timeout, connect_future)
        .await
        .map_err(|_| format!("SOCKS5 server ({}) connect timeout", socks_addr))?
        .map_err(|e| format!("Failed to connect to SOCKS5 ({}): {}", socks_addr, e))?;

    let _ = stream.set_nodelay(true);

    // 1. Method selection: SOCKSv5 (0x05), 1 method (0x01), NO AUTH (0x00)
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .map_err(|e| format!("SOCKS5 auth request error: {}", e))?;

    let mut auth_resp = [0u8; 2];
    stream
        .read_exact(&mut auth_resp)
        .await
        .map_err(|e| format!("SOCKS5 auth response error: {}", e))?;

    if auth_resp[0] != 0x05 || auth_resp[1] != 0x00 {
        return Err(format!(
            "SOCKS5 authentication failed or unsupported: {:?}",
            auth_resp
        ));
    }

    // 2. Connect request: VER=5, CMD=1 (CONNECT), RSV=0, ATYP...
    let mut req = Vec::with_capacity(32);
    req.extend_from_slice(&[0x05, 0x01, 0x00]);

    if let Ok(ip) = target_host.parse::<IpAddr>() {
        match ip {
            IpAddr::V4(ipv4) => {
                req.push(0x01); // ATYP: IPv4
                req.extend_from_slice(&ipv4.octets());
            }
            IpAddr::V6(ipv6) => {
                req.push(0x04); // ATYP: IPv6
                req.extend_from_slice(&ipv6.octets());
            }
        }
    } else {
        let host_bytes = target_host.as_bytes();
        if host_bytes.len() > 255 {
            return Err("Target hostname too long for SOCKS5".to_string());
        }
        req.push(0x03); // ATYP: DOMAINNAME
        req.push(host_bytes.len() as u8);
        req.extend_from_slice(host_bytes);
    }
    req.extend_from_slice(&target_port.to_be_bytes());

    stream
        .write_all(&req)
        .await
        .map_err(|e| format!("SOCKS5 connect request error: {}", e))?;

    // 3. Read reply: VER=5, REP=0, RSV=0, ATYP...
    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|e| format!("SOCKS5 connect reply error: {}", e))?;

    if header[0] != 0x05 {
        return Err(format!("Invalid SOCKS5 version in reply: {}", header[0]));
    }
    if header[1] != 0x00 {
        let err_msg = match header[1] {
            1 => "general SOCKS server failure",
            2 => "connection not allowed by ruleset",
            3 => "network unreachable",
            4 => "host unreachable",
            5 => "connection refused",
            6 => "TTL expired",
            7 => "command not supported",
            8 => "address type not supported",
            _ => "unknown SOCKS error",
        };
        return Err(format!(
            "SOCKS5 connect to {}:{} failed: {} (status code {})",
            target_host, target_port, err_msg, header[1]
        ));
    }

    // Skip bound address
    match header[3] {
        0x01 => {
            let mut skip = [0u8; 6];
            stream.read_exact(&mut skip).await.map_err(|e| e.to_string())?;
        }
        0x03 => {
            let len = stream.read_u8().await.map_err(|e| e.to_string())? as usize;
            let mut skip = vec![0u8; len + 2];
            stream.read_exact(&mut skip).await.map_err(|e| e.to_string())?;
        }
        0x04 => {
            let mut skip = [0u8; 18];
            stream.read_exact(&mut skip).await.map_err(|e| e.to_string())?;
        }
        _ => return Err(format!("Unknown address type in SOCKS5 reply: {}", header[3])),
    }

    Ok(stream)
}
