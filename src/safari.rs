//! Safari uses the macOS network proxy. Inspect it; never silently change it.
use crate::config::Config;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct SystemProxyInfo {
    pub ready: bool,
    pub message: String,
    pub http_port: u16,
    pub socks_port: u16,
}

pub fn setup_help(cfg: &Config) -> String {
    format!("macOS 시스템 설정 → 네트워크 → 사용 중인 연결 → 세부사항 → 프록시에서 HTTP와 HTTPS를 모두 127.0.0.1:{}로 설정하거나 SOCKS를 127.0.0.1:{}로 설정하세요. 자동 프록시 검색과 PAC는 끄세요. 이 설정은 Safari 외 다른 앱에도 적용되며, 프록시 종료 후에는 직접 원래 설정으로 돌려야 합니다. Safari는 기존 프로필과 시스템 프록시 제외 목록을 사용합니다.", cfg.http_port, cfg.socks_port)
}

// scutil nests arrays, per-interface dictionaries and supplemental rules.
// Keep dictionaries separate so a nested enabled proxy cannot masquerade as
// the default configuration or hide a different proxy for an interface/domain.
fn proxy_sections(text: &str) -> Result<Vec<BTreeMap<String, String>>, String> {
    let mut stack: Vec<BTreeMap<String, String>> = Vec::new();
    let mut root = None;
    let mut nested = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.ends_with('{') {
            stack.push(BTreeMap::new());
        } else if line == "}" {
            let section = stack
                .pop()
                .ok_or("시스템 프록시 응답 형식을 확인할 수 없습니다.")?;
            if stack.is_empty() {
                if root.replace(section).is_some() {
                    return Err("시스템 프록시 응답에 기본 설정이 여러 개 있습니다.".into());
                }
            } else if section.keys().any(|key| {
                matches!(
                    key.as_str(),
                    "HTTPEnable"
                        | "HTTPSEnable"
                        | "SOCKSEnable"
                        | "ProxyAutoConfigEnable"
                        | "ProxyAutoDiscoveryEnable"
                )
            }) {
                nested.push(section);
            }
        } else if let Some((key, value)) = line.split_once(" : ") {
            if let Some(section) = stack.last_mut() {
                if section.insert(key.to_owned(), value.to_owned()).is_some() {
                    return Err("시스템 프록시 응답에 중복 항목이 있습니다.".into());
                }
            }
        }
    }
    if !stack.is_empty() {
        return Err("시스템 프록시 응답이 완전하지 않습니다.".into());
    }
    let mut sections = vec![root.ok_or("시스템 프록시 설정을 읽을 수 없습니다.")?];
    sections.extend(nested);
    Ok(sections)
}

fn enabled(section: &BTreeMap<String, String>, key: &str) -> bool {
    section.get(key).map(String::as_str) == Some("1")
}

fn matches_pk(section: &BTreeMap<String, String>, prefix: &str, port: u16) -> bool {
    enabled(section, &format!("{prefix}Enable"))
        && section.get(&format!("{prefix}Proxy")).map(String::as_str) == Some("127.0.0.1")
        && section
            .get(&format!("{prefix}Port"))
            .and_then(|value| value.parse::<u16>().ok())
            == Some(port)
}

pub fn validate_proxy_output(text: &str, cfg: &Config) -> Result<bool, String> {
    let sections = proxy_sections(text)?;
    let mut needs_http = false;
    for (index, section) in sections.iter().enumerate() {
        if enabled(section, "ProxyAutoConfigEnable") || enabled(section, "ProxyAutoDiscoveryEnable")
        {
            return Err("자동 프록시 설정(PAC/자동 검색)이 켜져 있어 Safari의 PK 경로를 확인할 수 없습니다.".into());
        }
        if index > 0
            && !["HTTPEnable", "HTTPSEnable", "SOCKSEnable"]
                .iter()
                .any(|key| enabled(section, key))
        {
            continue;
        }
        let socks = matches_pk(section, "SOCKS", cfg.socks_port);
        for prefix in ["HTTP", "HTTPS"] {
            if enabled(section, &format!("{prefix}Enable")) {
                if !matches_pk(section, prefix, cfg.http_port) {
                    return Err(format!(
                        "시스템 {prefix} 프록시가 PK의 127.0.0.1:{}를 가리키지 않습니다.",
                        cfg.http_port
                    ));
                }
                needs_http = true;
            } else if !socks {
                return Err(format!(
                    "시스템 {prefix}/SOCKS 프록시가 PK를 가리키지 않습니다."
                ));
            }
        }
    }
    Ok(needs_http)
}

pub fn inspect(cfg: &Config) -> Result<bool, String> {
    if std::env::consts::OS != "macos" {
        return Err("Safari는 macOS에서만 지원합니다.".into());
    }
    let output = std::process::Command::new("/usr/sbin/scutil")
        .arg("--proxy")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("시스템 프록시 확인 실패: {error}"))?;
    if !output.status.success() {
        return Err("scutil에서 시스템 프록시를 읽지 못했습니다.".into());
    }
    validate_proxy_output(&String::from_utf8_lossy(&output.stdout), cfg)
}

pub fn info(cfg: &Config) -> SystemProxyInfo {
    let result = inspect(cfg);
    SystemProxyInfo {
        ready: result.is_ok(),
        message: match result {
            Ok(_) => "기본 및 별도로 지정된 시스템 프록시가 PK를 가리킵니다. Safari는 기존 프로필과 시스템 프록시 제외 목록을 사용합니다.".into(),
            Err(error) if std::env::consts::OS == "macos" => format!("{error} {}", setup_help(cfg)),
            Err(error) => error,
        },
        http_port: cfg.http_port,
        socks_port: cfg.socks_port,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_pk_proxy_for_both_web_schemes() {
        let cfg = Config::default();
        let http = "<dictionary> {\nHTTPEnable : 1\nHTTPProxy : 127.0.0.1\nHTTPPort : 3128\nHTTPSEnable : 1\nHTTPSProxy : 127.0.0.1\nHTTPSPort : 3128\n}";
        assert_eq!(validate_proxy_output(http, &cfg), Ok(true));
        assert!(
            validate_proxy_output(&http.replace("HTTPSPort : 3128", "HTTPSPort : 9999"), &cfg)
                .is_err()
        );
        assert!(
            validate_proxy_output(&http.replace("HTTPSEnable : 1", "HTTPSEnable : 0"), &cfg)
                .is_err()
        );
        let socks = "<dictionary> {\nSOCKSEnable : 1\nSOCKSProxy : 127.0.0.1\nSOCKSPort : 1080\n}";
        assert_eq!(validate_proxy_output(socks, &cfg), Ok(false));
        assert!(validate_proxy_output(&socks.replace("1080", "11080"), &cfg).is_err());
        assert_eq!(
            validate_proxy_output(
                &socks.replace("1080", "11080"),
                &Config {
                    socks_port: 11080,
                    ..cfg
                }
            ),
            Ok(false)
        );
    }
    #[test]
    fn rejects_pac_scoped_mismatch_and_incomplete_output() {
        let cfg = Config::default();
        let root = "<dictionary> {\nSOCKSEnable : 1\nSOCKSProxy : 127.0.0.1\nSOCKSPort : 1080\n";
        assert!(
            validate_proxy_output(&format!("{root}ProxyAutoConfigEnable : 1\n}}"), &cfg).is_err()
        );
        let scoped = "__SCOPED__ : <dictionary> {\nen0 : <dictionary> {\nSOCKSEnable : 1\nSOCKSProxy : 127.0.0.1\nSOCKSPort : 1080\n}\n}\n}";
        assert_eq!(
            validate_proxy_output(&format!("{root}{scoped}"), &cfg),
            Ok(false)
        );
        assert!(
            validate_proxy_output(&format!("{root}{}", scoped.replace("1080", "9999")), &cfg)
                .is_err()
        );
        assert!(validate_proxy_output(root, &cfg).is_err());
        assert!(validate_proxy_output("<dictionary> {\n}", &cfg).is_err());
    }
}
