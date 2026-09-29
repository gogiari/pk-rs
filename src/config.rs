use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_ssh_target")]
    pub ssh_target: String,

    #[serde(default = "default_http_port")]
    pub http_port: u16,

    #[serde(default = "default_socks_port")]
    pub socks_port: u16,

    #[serde(default = "default_web_port")]
    pub web_port: u16,

    #[serde(default = "default_no_proxy")]
    pub no_proxy: String,

    #[serde(default)]
    pub ssh_password: Option<String>,

    #[serde(default)]
    pub ssh_key_path: Option<String>,

    #[serde(default)]
    pub auto_connect: bool,

    #[serde(default = "default_true")]
    pub auto_open_browser: bool,
}

fn default_ssh_target() -> String {
    String::new()
}

fn default_http_port() -> u16 {
    3128
}

fn default_socks_port() -> u16 {
    1080
}

fn default_web_port() -> u16 {
    8253
}

fn default_no_proxy() -> String {
    "localhost,127.0.0.1,::1,100.64.0.0/10,tailscale.com,*.tailscale.com".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ssh_target: default_ssh_target(),
            http_port: default_http_port(),
            socks_port: default_socks_port(),
            web_port: default_web_port(),
            no_proxy: default_no_proxy(),
            ssh_password: None,
            ssh_key_path: None,
            auto_connect: false,
            auto_open_browser: true,
        }
    }
}

pub fn config_path() -> PathBuf {
    if let Ok(dir) = std::env::var("PK_CONFIG_DIR") {
        return PathBuf::from(dir).join("config.toml");
    }
    if let Some(mut path) = get_base_config_dir() {
        path.push("pk");
        path.push("config.toml");
        return path;
    }
    PathBuf::from("config.toml")
}

fn get_base_config_dir() -> Option<PathBuf> {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return Some(PathBuf::from(appdata));
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        return Some(PathBuf::from(home).join(".config"));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home).join(".config"));
    }
    None
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(cfg) = toml::from_str::<Config>(&content) {
                    return cfg;
                }
            }
        }
        Config::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, content).map_err(|e| e.to_string())?;
        Ok(())
    }
}
