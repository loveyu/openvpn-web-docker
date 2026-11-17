use serde::{Deserialize, Serialize};
use std::{collections::HashSet, net::Ipv4Addr};

pub const WEB_PORT: u16 = 80;
pub const HTTP_PORT: u16 = 8888;
pub const SOCKS5_PORT: u16 = 1080;
pub const SSH_PORT: u16 = 22;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub vpn: Vpn,
    pub http: Proxy,
    pub socks5: Socks,
    pub sshd: Ssh,
    pub health: Health,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigDocument {
    pub active_profile: String,
    pub profiles: Vec<Profile>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub settings: Config,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "默认 VPN".into(),
            settings: Config::default(),
        }
    }
}

impl Default for ConfigDocument {
    fn default() -> Self {
        let profile = Profile::default();
        Self {
            active_profile: profile.id.clone(),
            profiles: vec![profile],
        }
    }
}

impl ConfigDocument {
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = HashSet::new();
        for profile in &self.profiles {
            if uuid::Uuid::parse_str(&profile.id).is_err()
                || !ids.insert(&profile.id)
                || profile.name.trim().is_empty()
            {
                return Err("VPN 配置的 ID 必须唯一且有效，名称不能为空".into());
            }
            profile.settings.validate()?;
        }
        if !ids.contains(&self.active_profile) {
            return Err("请选择有效的 VPN 配置".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Vpn {
    pub config: String,
    pub endpoints: Vec<Endpoint>,
    pub selected_endpoint: String,
    pub selection_mode: SelectionMode,
    pub username: String,
    pub password: String,
    pub ca: String,
    pub cert: String,
    pub key: String,
    pub tls_auth: String,
    pub tls_crypt: String,
    pub tls_auth_direction: u8,
    pub dns: Vec<Ipv4Addr>,
    pub dns_mode: DnsMode,
    pub httpdns_url: String,
    pub httpdns_ips: Vec<Ipv4Addr>,
    pub httpdns_protocol: HttpDnsProtocol,
    pub autoconnect: bool,
}

impl Default for Vpn {
    fn default() -> Self {
        Self {
            config: String::new(),
            endpoints: Vec::new(),
            selected_endpoint: String::new(),
            selection_mode: SelectionMode::Auto,
            username: String::new(),
            password: String::new(),
            ca: String::new(),
            cert: String::new(),
            key: String::new(),
            tls_auth: String::new(),
            tls_crypt: String::new(),
            tls_auth_direction: 1,
            dns: vec![Ipv4Addr::new(1, 1, 1, 1)],
            dns_mode: DnsMode::Manual,
            httpdns_url: "https://doh.pub/dns-query".into(),
            httpdns_ips: vec![Ipv4Addr::new(1, 12, 12, 12), Ipv4Addr::new(120, 53, 53, 53)],
            httpdns_protocol: HttpDnsProtocol::Doh,
            autoconnect: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SelectionMode {
    #[default]
    Auto,
    Manual,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HttpDnsProtocol {
    #[default]
    Doh,
    Json,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Health {
    pub enabled: bool,
    pub kind: CheckKind,
    pub target: String,
    pub port: u16,
    pub url: String,
    pub http_method: HttpMethod,
    pub expected_status: u16,
    pub interval_secs: u64,
    pub timeout_secs: u64,
    pub failure_threshold: u32,
    pub ping_count: u8,
    pub action: HealthAction,
    pub action_cooldown_secs: u64,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            enabled: false,
            kind: CheckKind::Http,
            target: "1.1.1.1".into(),
            port: 443,
            url: "https://www.qq.com".into(),
            http_method: HttpMethod::Get,
            expected_status: 200,
            interval_secs: 30,
            timeout_secs: 5,
            failure_threshold: 3,
            ping_count: 1,
            action: HealthAction::Restart,
            action_cooldown_secs: 120,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckKind {
    Ping,
    Tcp,
    Http,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Head,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HealthAction {
    None,
    Reconnect,
    Restart,
    Reapply,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub protocol: Protocol,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Udp,
    Tcp,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DnsMode {
    #[default]
    Manual,
    Pushed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Proxy {
    pub enabled: bool,
    pub username: String,
    pub password: String,
    pub authentication: Authentication,
}

impl Default for Proxy {
    fn default() -> Self {
        Self {
            enabled: true,
            username: String::new(),
            password: String::new(),
            authentication: Authentication::None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Socks {
    pub enabled: bool,
    pub username: String,
    pub password: String,
    pub authentication: Authentication,
}

impl Default for Socks {
    fn default() -> Self {
        Self {
            enabled: false,
            username: String::new(),
            password: String::new(),
            authentication: Authentication::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Authentication {
    #[default]
    None,
    Password,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ssh {
    pub enabled: bool,
    pub username: String,
    pub password: String,
    pub authorized_keys: String,
    pub sudo: bool,
}

impl Default for Ssh {
    fn default() -> Self {
        Self {
            enabled: false,
            username: "work".into(),
            password: String::new(),
            authorized_keys: String::new(),
            sudo: false,
        }
    }
}

fn unix_username(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 32
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'-')
        && ![
            "root",
            "daemon",
            "bin",
            "sys",
            "sync",
            "games",
            "man",
            "lp",
            "mail",
            "news",
            "uucp",
            "proxy",
            "www-data",
            "backup",
            "list",
            "irc",
            "_apt",
            "nobody",
            "sshd",
            "tinyproxy",
            "messagebus",
            "systemd-network",
            "systemd-timesync",
        ]
        .contains(&name)
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.health.interval_secs == 0
            || self.health.timeout_secs == 0
            || self.health.timeout_secs > 60
            || self.health.failure_threshold == 0
            || self.health.ping_count == 0
            || self.health.port == 0
            || !(100..=599).contains(&self.health.expected_status)
        {
            return Err("健康检查的间隔、失败阈值、次数和端口必须大于 0；超时为 1–60 秒；HTTP 状态码为 100–599".into());
        }
        if !(1..=86400).contains(&self.health.action_cooldown_secs) {
            return Err("健康检查恢复动作的冷却时间必须为 1–86400 秒".into());
        }
        let mut endpoint_ids = HashSet::new();
        for endpoint in &self.vpn.endpoints {
            if endpoint.id.is_empty()
                || !endpoint_ids.insert(&endpoint.id)
                || endpoint.host.trim().is_empty()
                || endpoint.port == 0
            {
                return Err("Endpoint 的 ID 必须唯一，地址不能为空，端口必须为 1–65535".into());
            }
        }
        if !self.vpn.endpoints.is_empty() && !endpoint_ids.contains(&self.vpn.selected_endpoint) {
            return Err("请选择一个有效的 OpenVPN endpoint".into());
        }
        for (name, username) in [
            ("HTTP", &self.http.username),
            ("SOCKS5", &self.socks5.username),
        ] {
            if !username.is_empty() && !unix_username(username) {
                return Err(format!(
                    "{name} 用户名必须以小写字母开头，只包含小写字母、数字、下划线或短横线，最长 32 位，且不能使用系统账户"
                ));
            }
        }
        for (name, mode, username, password) in [
            (
                "HTTP",
                self.http.authentication,
                &self.http.username,
                &self.http.password,
            ),
            (
                "SOCKS5",
                self.socks5.authentication,
                &self.socks5.username,
                &self.socks5.password,
            ),
        ] {
            if mode == Authentication::Password && (username.is_empty() || password.is_empty()) {
                return Err(format!("{name} 选择密码认证时必须填写用户名和密码"));
            }
            if mode == Authentication::None && (!username.is_empty() || !password.is_empty()) {
                return Err(format!("{name} 无认证模式不能保存认证凭据"));
            }
        }
        if self.http.password.chars().any(char::is_whitespace) {
            return Err("HTTP 代理密码不能包含空白字符（Tinyproxy 配置格式限制）".into());
        }
        if !unix_username(&self.sshd.username) {
            return Err("SSH 用户名无效或属于系统保留账户".into());
        }
        if !self.socks5.username.is_empty() && self.socks5.username == self.sshd.username {
            return Err("SOCKS5 与 SSH 必须使用不同账户".into());
        }
        if self.sshd.enabled
            && self.sshd.password.is_empty()
            && self.sshd.authorized_keys.trim().is_empty()
        {
            return Err("启用 SSH 时至少配置密码或一个公钥".into());
        }
        if self.vpn.username.is_empty() != self.vpn.password.is_empty() {
            return Err("OpenVPN 用户名和密码必须同时填写或同时留空".into());
        }
        for value in [
            &self.http.password,
            &self.socks5.password,
            &self.sshd.password,
            &self.vpn.username,
            &self.vpn.password,
        ] {
            if value.contains(['\n', '\r', '\0']) {
                return Err("用户名和密码不能包含换行或 NUL 字符".into());
            }
        }
        if self.vpn.dns_mode == DnsMode::Manual && self.vpn.dns.is_empty() {
            return Err("手动 DNS 模式至少需要一个 IPv4 地址".into());
        }
        if self.vpn.httpdns_ips.is_empty() {
            return Err("HTTPDNS 至少需要一个用于直连的 IPv4 地址".into());
        }
        if self.vpn.tls_auth_direction > 1 {
            return Err("tls-auth 方向只能为 0 或 1".into());
        }
        if !self.vpn.tls_auth.is_empty() && !self.vpn.tls_crypt.is_empty() {
            return Err("tls-auth 和 tls-crypt 只能选择一种".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_keep_optional_services_disabled() {
        let config = Config::default();
        assert!(config.validate().is_ok());
        assert!(!config.sshd.enabled && !config.socks5.enabled);
    }
    #[test]
    fn rejects_missing_authentication() {
        let mut config = Config::default();
        config.sshd.enabled = true;
        assert!(config.validate().is_err());
        config.sshd.authorized_keys = "ssh-ed25519 AAAA test".into();
        assert!(config.validate().is_ok());
        config.socks5.username = "proxyuser".into();
        assert!(config.validate().is_err());
        config.socks5.username.clear();
        config.socks5.authentication = Authentication::Password;
        assert!(config.validate().is_err());
    }
    #[test]
    fn prevents_password_file_injection_and_shared_accounts() {
        let mut config = Config::default();
        config.sshd.password = "value\nroot:injected".into();
        assert!(config.validate().is_err());
        config.sshd.password.clear();
        config.socks5.username = "work".into();
        config.socks5.password = "secret".into();
        assert!(config.validate().is_err());
    }
}
