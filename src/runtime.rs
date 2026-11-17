use crate::{
    config::{
        self, Config, DnsMode, HTTP_PORT, Protocol, RouteMode, SOCKS5_PORT, SSH_PORT,
        SelectionMode, WEB_PORT,
    },
    network::{self, Result},
    storage,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    net::Ipv4Addr,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    task::JoinSet,
};

mod health;

#[derive(Clone, Deserialize, Serialize)]
struct Active {
    id: String,
    name: String,
    settings: Config,
}
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
struct Remote {
    ip: Ipv4Addr,
    port: u16,
    protocol: String,
    host: String,
    endpoint_id: String,
}

struct Runtime {
    data: PathBuf,
    run: PathBuf,
}
impl Runtime {
    fn saved(&self) -> Result<Active> {
        let document = storage::load(&self.data)?;
        let profile = document
            .profiles
            .into_iter()
            .find(|profile| profile.id == document.active_profile)
            .ok_or("当前 VPN 配置不存在")?;
        Ok(Active {
            id: profile.id,
            name: profile.name,
            settings: profile.settings,
        })
    }
    fn active(&self) -> Result<Active> {
        let path = self.run.join("applied.json");
        if path.exists() {
            Ok(serde_json::from_slice(&fs::read(path)?)?)
        } else {
            self.saved()
        }
    }
    fn write(&self, path: &Path, value: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        storage::private_write(path, value)?;
        Ok(())
    }
    fn json<T: Serialize>(&self, name: &str, value: &T) -> Result<()> {
        self.write(&self.run.join(name), &serde_json::to_vec(value)?)
    }
    fn read(&self, name: &str, default: Value) -> Result<Value> {
        let path = self.run.join(name);
        if path.exists() {
            Ok(serde_json::from_slice(&fs::read(path)?)?)
        } else {
            Ok(default)
        }
    }
    fn endpoints(&self) -> Result<Vec<Remote>> {
        let path = self.run.join("endpoints.json");
        if path.exists() {
            Ok(serde_json::from_slice(&fs::read(path)?)?)
        } else {
            Ok(vec![])
        }
    }
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}
async fn supervisor(action: &str, name: Option<&str>) -> Result<String> {
    let mut values = args(&["-c", "/etc/supervisor/supervisord.conf", action]);
    if let Some(name) = name {
        values.push(name.into());
    }
    let timeout = if action == "status" { 2 } else { 40 };
    let response = network::output("supervisorctl", &values, None, timeout).await?;
    if !response.status.success() && action != "status" {
        return Err(format!(
            "Supervisor：{}{}",
            String::from_utf8_lossy(&response.stdout),
            String::from_utf8_lossy(&response.stderr)
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&response.stdout).into_owned())
}
async fn processes() -> Result<Vec<Value>> {
    let text = supervisor("status", None).await?;
    Ok(text.lines().filter_map(|line|{let words=line.split_whitespace().collect::<Vec<_>>();if words.len()<2{return None}let pid=if words.get(2)==Some(&"pid"){words.get(3).and_then(|value|value.trim_end_matches(',').parse::<u32>().ok()).unwrap_or(0)}else{0};Some(json!({"name":words[0],"state":words[1],"pid":pid,"description":words[2..].join(" ")}))}).collect())
}
async fn stop(name: &str) -> Result<()> {
    let state = processes()
        .await?
        .into_iter()
        .find(|item| item["name"] == name);
    if state
        .is_some_and(|item| !matches!(item["state"].as_str(), Some("STOPPED" | "EXITED" | "FATAL")))
    {
        supervisor("stop", Some(name)).await?;
    }
    Ok(())
}

fn words(line: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut escape = false;
    let mut started = false;
    for c in line.chars() {
        if escape {
            word.push(c);
            escape = false;
            started = true;
            continue;
        }
        if c == '\\' && quote != Some('\'') {
            escape = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None
            } else {
                word.push(c)
            }
            started = true;
            continue;
        }
        if c == '#' || c == ';' {
            break;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
            started = true
        } else if c.is_whitespace() {
            if started {
                out.push(std::mem::take(&mut word));
                started = false
            }
        } else {
            word.push(c);
            started = true
        }
    }
    if quote.is_some() || escape {
        return Err("OpenVPN 配置的引号或转义未闭合".into());
    }
    if started {
        out.push(word)
    }
    Ok(out)
}

type ParsedConfig = (Vec<String>, Vec<(String, u16, String, String)>);
fn parse(vpn: &config::Vpn) -> Result<ParsedConfig> {
    if vpn.config.trim().is_empty() && vpn.endpoints.is_empty() {
        return Ok((vec![], vec![]));
    }
    let text = if vpn.config.trim().is_empty() {
        "client\nremote-cert-tls server"
    } else {
        &vpn.config
    };
    let blocked = [
        "config",
        "cd",
        "chroot",
        "daemon",
        "inetd",
        "log",
        "log-append",
        "syslog",
        "writepid",
        "status",
        "status-version",
        "dev",
        "dev-type",
        "dev-node",
        "user",
        "group",
        "script-security",
        "up",
        "down",
        "route-up",
        "route-pre-down",
        "ipchange",
        "tls-verify",
        "auth-user-pass-verify",
        "client-connect",
        "client-disconnect",
        "learn-address",
        "plugin",
        "client-config-dir",
        "tmp-dir",
        "askpass",
        "route-noexec",
        "ifconfig-noexec",
        "secret",
        "pkcs11-providers",
        "providers",
        "engine",
        "capath",
        "tls-export-cert",
    ];
    let mut kept = Vec::new();
    let mut remotes = Vec::<(String, Option<u16>, Option<String>)>::new();
    let mut inline: Option<String> = None;
    let mut port = 1194;
    let mut protocol = "udp".to_owned();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(tag) = &inline {
            kept.push(line.into());
            if trimmed == format!("</{tag}>") {
                inline = None
            }
            continue;
        }
        if trimmed.starts_with('<') {
            if ![
                "<ca>",
                "<cert>",
                "<key>",
                "<tls-auth>",
                "<tls-crypt>",
                "<extra-certs>",
            ]
            .contains(&trimmed)
            {
                return Err(format!("不支持 {trimmed}，请使用平面配置和内联证书").into());
            }
            let tag = trimmed.trim_start_matches('<').trim_end_matches('>');
            let value = match tag {
                "ca" => &vpn.ca,
                "cert" => &vpn.cert,
                "key" => &vpn.key,
                "tls-auth" => &vpn.tls_auth,
                "tls-crypt" => &vpn.tls_crypt,
                _ => "",
            };
            if !value.is_empty() {
                return Err(format!("{tag} 已内联，请勿在表单重复填写").into());
            }
            inline = Some(tag.into());
            kept.push(line.into());
            continue;
        }
        let tokens = words(line)?;
        if tokens.is_empty() {
            kept.push(line.into());
            continue;
        }
        let option = tokens[0].trim_start_matches("--");
        if vpn.route_mode == RouteMode::All {
            // 全隧道统一生成路由，避免原文的静态路由、禁用指令或过滤器绕开此模式。
            if matches!(
                option,
                "route"
                    | "route-ipv6"
                    | "route-gateway"
                    | "route-ipv6-gateway"
                    | "route-metric"
                    | "route-delay"
                    | "route-method"
                    | "route-nopull"
                    | "route-noexec"
                    | "redirect-gateway"
                    | "redirect-private"
                    | "max-routes"
            ) {
                continue;
            }
            if option == "pull-filter"
                && tokens
                    .get(1)
                    .is_some_and(|action| matches!(action.as_str(), "ignore" | "reject"))
                && tokens.get(2).is_some_and(|filter| {
                    ["route", "redirect-gateway", "redirect-private"]
                        .iter()
                        .any(|directive| {
                            directive.starts_with(filter) || filter.starts_with(directive)
                        })
                })
            {
                continue;
            }
        }
        if option == "dev" && tokens.len() == 2 && matches!(tokens[1].as_str(), "tun" | "tun0") {
            continue;
        }
        if blocked.contains(&option) || option.starts_with("management") {
            return Err(format!(
                "OpenVPN 参数 {option} 由容器管理或不允许执行外部程序，请移除该行"
            )
            .into());
        }
        match option {
            "remote" => {
                if !vpn.endpoints.is_empty() {
                    continue;
                }
                if !(2..=4).contains(&tokens.len()) {
                    return Err("remote 格式：remote 主机 [端口] [udp|tcp-client]".into());
                }
                remotes.push((
                    network::domain(&tokens[1])?,
                    tokens.get(2).map(|value| value.parse()).transpose()?,
                    tokens.get(3).cloned(),
                ));
            }
            "port" | "rport" => port = tokens.get(1).ok_or("缺少远端端口")?.parse()?,
            "proto" => protocol = tokens.get(1).ok_or("缺少 proto")?.clone(),
            "auth-user-pass" => {
                if vpn.username.is_empty() || vpn.password.is_empty() {
                    return Err("请在表单填写 VPN 用户名和密码".into());
                }
            }
            "ca" | "cert" | "key" | "tls-auth" | "tls-crypt" => {
                let value = match option {
                    "ca" => &vpn.ca,
                    "cert" => &vpn.cert,
                    "key" => &vpn.key,
                    "tls-auth" => &vpn.tls_auth,
                    _ => &vpn.tls_crypt,
                };
                if value.is_empty() {
                    return Err(format!("{option} 引用了外部文件，请把完整内容填入证书表单").into());
                }
            }
            "key-direction" if !vpn.tls_auth.is_empty() => {}
            _ => kept.push(line.into()),
        }
    }
    if let Some(tag) = inline {
        return Err(format!("内联证书 {tag} 缺少结束标签").into());
    }
    let mut normalized = Vec::new();
    if !vpn.endpoints.is_empty() {
        for endpoint in &vpn.endpoints {
            network::domain(&endpoint.host)?;
            if vpn.selection_mode == SelectionMode::Auto || endpoint.id == vpn.selected_endpoint {
                normalized.push((
                    endpoint.host.clone(),
                    endpoint.port,
                    match endpoint.protocol {
                        Protocol::Udp => "udp",
                        Protocol::Tcp => "tcp",
                    }
                    .into(),
                    endpoint.id.clone(),
                ))
            }
        }
    } else {
        for (host, remote_port, remote_proto) in remotes {
            let proto = remote_proto.unwrap_or_else(|| protocol.clone());
            if !matches!(
                proto.as_str(),
                "udp" | "udp4" | "tcp" | "tcp-client" | "tcp4" | "tcp4-client"
            ) {
                return Err("VPN 远端目前支持 UDP/TCP IPv4 客户端".into());
            }
            let port = remote_port.unwrap_or(port);
            if port == 0 {
                return Err("远端端口必须为 1–65535".into());
            }
            normalized.push((
                host,
                port,
                if proto.starts_with("udp") {
                    "udp"
                } else {
                    "tcp"
                }
                .into(),
                String::new(),
            ));
        }
    }
    if normalized.is_empty() {
        return Err("请配置至少一个 OpenVPN remote 或 endpoint".into());
    }
    if vpn.route_mode == RouteMode::All {
        // 只忽略下发的静态路由和重定向；保留下发的 route-gateway 以支持 subnet 拓扑。
        kept.splice(
            0..0,
            args(&[
                "pull-filter ignore \"route \"",
                "pull-filter ignore \"route-ipv6 \"",
                "pull-filter ignore \"redirect-gateway\"",
                "pull-filter ignore \"redirect-private\"",
            ]),
        );
        kept.push("redirect-gateway def1".into());
    }
    // DNS 由界面统一管理；优先接收下发值，避免原文的 ignore/reject 过滤器使自动模式失效。
    kept.insert(0, "pull-filter accept \"dhcp-option DNS \"".into());
    Ok((kept, normalized))
}

fn validate(config: &Config) -> Result<()> {
    parse(&config.vpn)?;
    network::http_target(&config.vpn.httpdns_url)?;
    if config.health.enabled {
        match config.health.kind {
            config::CheckKind::Http => {
                network::http_target(&config.health.url)?;
            }
            _ => {
                network::domain(&config.health.target)?;
            }
        }
    }
    for value in [
        &config.vpn.ca,
        &config.vpn.cert,
        &config.vpn.key,
        &config.vpn.tls_auth,
        &config.vpn.tls_crypt,
    ] {
        if !value.is_empty() && (!value.contains("-----BEGIN ") || !value.contains("-----END ")) {
            return Err("证书或密钥必须填写完整 PEM/OpenVPN 静态密钥内容".into());
        }
    }
    Ok(())
}

async fn firewall(rt: &Runtime, config: &Active, endpoints: &[Remote]) -> Result<()> {
    let path = rt.run.join("network.json");
    let info = if path.exists() {
        serde_json::from_slice::<Value>(&fs::read(path)?)?
    } else {
        let routes: Value = serde_json::from_str(
            &network::command("ip", &args(&["-j", "-4", "route", "show", "default"]), None).await?,
        )?;
        let route = routes
            .as_array()
            .and_then(|items| items.first())
            .ok_or("需要具有 IPv4 默认网关的独立 bridge 网络，不支持 host/none 模式")?;
        let interface = route["dev"].as_str().ok_or("默认网卡不存在")?;
        let gateway = route["gateway"].as_str().ok_or("默认网关不存在")?;
        let addresses: Value = serde_json::from_str(
            &network::command(
                "ip",
                &args(&["-j", "-4", "address", "show", "dev", interface]),
                None,
            )
            .await?,
        )?;
        let address = &addresses[0]["addr_info"][0];
        let source = address["local"].as_str().ok_or("网卡没有 IPv4 地址")?;
        let prefix = address["prefixlen"].as_u64().ok_or("网卡没有网络前缀")? as u32;
        let ip = u32::from(source.parse::<Ipv4Addr>()?);
        let network = Ipv4Addr::from(
            ip & if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            },
        );
        let info = json!({"interface":interface,"gateway":gateway,"source":source,"subnet":format!("{network}/{prefix}")});
        rt.json("network.json", &info)?;
        info
    };
    let mut ports = vec![WEB_PORT];
    // 开发 Supervisor 复用管理连接的回程路由，不增加隧道外主动出站权限。
    if std::env::var("VPNWEB_DEV").as_deref() == Ok("1") {
        ports.push(9001);
    }
    for (enabled, port) in [
        (config.settings.http.enabled, HTTP_PORT),
        (config.settings.socks5.enabled, SOCKS5_PORT),
        (config.settings.sshd.enabled, SSH_PORT),
    ] {
        if enabled {
            ports.push(port)
        }
    }
    ports.sort();
    ports.dedup();
    let mut remote_entries = endpoints
        .iter()
        .map(|r| format!("{} . {} . {}", r.ip, r.protocol, r.port))
        .collect::<Vec<_>>();
    remote_entries.sort();
    remote_entries.dedup();
    let mut hosts = endpoints
        .iter()
        .map(|r| r.ip.to_string())
        .collect::<Vec<_>>();
    hosts.sort();
    hosts.dedup();
    let mut dns = network::providers()
        .into_iter()
        .flat_map(|provider| provider.ips.into_iter().map(|ip| (ip, 443)))
        .collect::<HashSet<_>>();
    let (_, port) = network::http_target(&config.settings.vpn.httpdns_url)?;
    dns.extend(config.settings.vpn.httpdns_ips.iter().map(|ip| (*ip, port)));
    let mut dns = dns
        .into_iter()
        .map(|(ip, port)| format!("{ip} . {port}"))
        .collect::<Vec<_>>();
    dns.sort();
    let elements = |items: Vec<String>| {
        if items.is_empty() {
            String::new()
        } else {
            format!("elements = {{ {} }}", items.join(", "))
        }
    };
    let exists = network::output("nft", &args(&["list", "table", "inet", "vpnweb"]), None, 5)
        .await?
        .status
        .success();
    let rules = format!(
        r#"{}
table inet vpnweb {{
 set vpn_endpoints {{ type ipv4_addr . inet_proto . inet_service; {} }}
 set vpn_hosts {{ type ipv4_addr; {} }}
 set httpdns_endpoints {{ type ipv4_addr . inet_service; {} }}
 chain reply_route {{ type route hook output priority mangle; policy accept; ct mark 0x1 meta mark set 0x1; }}
 chain input {{ type filter hook input priority filter; policy drop;
  iifname "lo" accept
  iifname "tun0" accept
  ct state established,related accept
  iifname "{}" tcp dport {{ {} }} ct mark set 0x1 accept
 }}
 chain output {{ type filter hook output priority filter; policy drop;
  oifname "lo" accept
  oifname "tun0" accept
  ct mark 0x1 accept
  meta skuid 0 ip daddr . meta l4proto . th dport @vpn_endpoints accept
  meta skuid 0 ip daddr . tcp dport @httpdns_endpoints accept
  meta skuid 0 ip daddr @vpn_hosts icmp type echo-request accept
 }}
 chain forward {{ type filter hook forward priority filter; policy drop; }}
}}
"#,
        if exists {
            "delete table inet vpnweb"
        } else {
            ""
        },
        elements(remote_entries),
        elements(hosts),
        elements(dns),
        info["interface"].as_str().unwrap(),
        ports
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    network::command("nft", &args(&["-f", "-"]), Some(rules.as_bytes())).await?;
    network::command(
        "ip",
        &args(&[
            "-4",
            "route",
            "replace",
            "table",
            "100",
            info["subnet"].as_str().unwrap(),
            "dev",
            info["interface"].as_str().unwrap(),
            "src",
            info["source"].as_str().unwrap(),
        ]),
        None,
    )
    .await?;
    network::command(
        "ip",
        &args(&[
            "-4",
            "route",
            "replace",
            "table",
            "100",
            "default",
            "via",
            info["gateway"].as_str().unwrap(),
            "dev",
            info["interface"].as_str().unwrap(),
        ]),
        None,
    )
    .await?;
    if !network::command("ip", &args(&["-4", "rule", "show"]), None)
        .await?
        .contains("fwmark 0x1 lookup 100")
    {
        network::command(
            "ip",
            &args(&[
                "-4", "rule", "add", "priority", "100", "fwmark", "0x1", "lookup", "100",
            ]),
            None,
        )
        .await?;
    }
    Ok(())
}

async fn vpn_state(rt: &Runtime) -> Value {
    let work = async {
        let mut sock =
            tokio::net::UnixStream::connect(rt.run.join("openvpn-management.sock")).await?;
        let mut banner = [0; 4096];
        if sock.read(&mut banner).await? == 0 {
            return Ok(json!({"state":"DISCONNECTED","ip":""}));
        }
        sock.write_all(b"state\n").await?;
        let mut content = Vec::new();
        loop {
            let mut buffer = [0; 4096];
            let n = sock.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            content.extend_from_slice(&buffer[..n]);
            if content.windows(5).any(|w| w == b"END\r\n") {
                break;
            }
        }
        for line in String::from_utf8_lossy(&content).lines() {
            let fields = line.split(',').collect::<Vec<_>>();
            if fields.len() >= 4 && fields[0].parse::<u64>().is_ok() {
                return Ok::<_, std::io::Error>(
                    json!({"state":fields[1],"detail":fields[2],"ip":fields[3],"since":fields[0].parse::<u64>().unwrap()}),
                );
            }
        }
        Ok(json!({"state":"DISCONNECTED","ip":""}))
    };
    tokio::time::timeout(Duration::from_secs(1), work)
        .await
        .ok()
        .and_then(std::result::Result::ok)
        .unwrap_or_else(|| json!({"state":"DISCONNECTED","ip":""}))
}

fn write_dns(rt: &Runtime, config: &Active, pushed: Option<Vec<String>>) -> Result<Value> {
    let previous = rt.read("dns.json", json!({}))?;
    let detected = pushed.clone().unwrap_or_else(|| {
        previous["pushed"]
            .as_array()
            .map(|v| {
                v.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    });
    let manual = config.settings.vpn.dns_mode == DnsMode::Manual;
    let effective = if manual {
        config
            .settings
            .vpn
            .dns
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    } else {
        detected.clone()
    };
    fs::write(
        "/etc/resolv.conf",
        if effective.is_empty() {
            "nameserver 127.0.0.1\n".into()
        } else {
            effective
                .iter()
                .map(|ip| format!("nameserver {ip}\n"))
                .collect::<String>()
        },
    )?;
    let value = json!({"mode":config.settings.vpn.dns_mode,"pushed":detected,"effective":effective,"source":if manual{"manual"}else if effective.is_empty(){"none"}else{"pushed"},"detected_at":if pushed.is_some(){json!(network::timestamp())}else{previous["detected_at"].clone()}});
    rt.json("dns.json", &value)?;
    Ok(value)
}

async fn render_vpn(rt: &Runtime, config: &Active) -> Result<Vec<Remote>> {
    let vpn = &config.settings.vpn;
    let (mut base, raw) = parse(vpn)?;
    rt.json("selected-endpoint.json", &Value::Null)?;
    rt.json("endpoint-probes.json", &json!([]))?;
    let mut dns_failures = Vec::new();
    let catalog = rt.read("httpdns.json", json!({}))?["results"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut cache: HashMap<String, Vec<Ipv4Addr>> = HashMap::new();
    let mut candidates = Vec::new();
    for (host, port, protocol, endpoint_id) in raw {
        let ips = if let Ok(ip) = host.parse::<Ipv4Addr>() {
            vec![ip]
        } else if let Some(ips) = cache.get(&host) {
            ips.clone()
        } else {
            let answer = match network::resolve(vpn, &host, &catalog).await {
                Ok(answer) => answer,
                Err(error) if vpn.selection_mode == SelectionMode::Auto => {
                    dns_failures.push(json!({"ip":host,"port":port,"protocol":protocol,"endpoint_id":endpoint_id,"ok":false,"latency_ms":null,"error":error.to_string()}));
                    continue;
                }
                Err(error) => return Err(error),
            };
            rt.json("httpdns-current.json", &answer)?;
            let ips = answer["answers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().parse::<Ipv4Addr>())
                .collect::<std::result::Result<Vec<_>, _>>()?;
            cache.insert(host.clone(), ips.clone());
            ips
        };
        for ip in ips {
            candidates.push(Remote {
                ip,
                port,
                protocol: protocol.clone(),
                host: host.clone(),
                endpoint_id: endpoint_id.clone(),
            })
        }
    }
    if !vpn.username.is_empty() {
        rt.write(
            &rt.run.join("vpn.auth"),
            format!("{}\n{}\n", vpn.username, vpn.password).as_bytes(),
        )?;
        base.push(format!(
            "auth-user-pass {}",
            rt.run.join("vpn.auth").display()
        ));
    }
    for (directive, value) in [
        ("ca", &vpn.ca),
        ("cert", &vpn.cert),
        ("key", &vpn.key),
        ("tls-auth", &vpn.tls_auth),
        ("tls-crypt", &vpn.tls_crypt),
    ] {
        if !value.is_empty() {
            let path = rt.run.join(format!("{directive}.pem"));
            rt.write(&path, value.as_bytes())?;
            base.push(format!("{directive} {}", path.display()));
            if directive == "tls-auth" {
                base.push(format!("key-direction {}", vpn.tls_auth_direction))
            }
        }
    }
    if candidates.is_empty() && !dns_failures.is_empty() {
        rt.json("endpoint-probes.json", &dns_failures)?;
        return Err("所有 endpoint 均无法通过 HTTPDNS 解析".into());
    }
    let endpoints = if !candidates.is_empty() && vpn.selection_mode == SelectionMode::Auto {
        firewall(rt, config, &candidates).await?;
        let selected = select_endpoint(rt, &base, candidates).await;
        let mut probes = rt.read("endpoint-probes.json", json!([]))?;
        probes.as_array_mut().unwrap().extend(dns_failures);
        rt.json("endpoint-probes.json", &probes)?;
        selected?
    } else {
        if let Some(first) = candidates.first() {
            rt.json("selected-endpoint.json", first)?;
        }
        candidates
    };
    for endpoint in &endpoints {
        base.push(format!(
            "remote {} {} {}",
            endpoint.ip,
            endpoint.port,
            if endpoint.protocol == "udp" {
                "udp4"
            } else {
                "tcp4-client"
            }
        ))
    }
    base.extend(args(&[
        "dev tun0",
        "nobind",
        "auth-nocache",
        "script-security 2",
        "up \"/usr/local/bin/openvpn-web runtime vpn-up\"",
        "up-restart",
        "management /run/vpnweb/openvpn-management.sock unix",
        "verb 3",
    ]));
    rt.write(
        &rt.run.join("openvpn.conf"),
        (base.join("\n") + "\n").as_bytes(),
    )?;
    Ok(endpoints)
}

async fn probe_endpoint(
    run: PathBuf,
    base: Vec<String>,
    remote: Remote,
    index: usize,
) -> Result<(Remote, Value)> {
    let mut latency = network::ping(&remote.ip.to_string(), 1, 1).await;
    let mut method = if latency.is_some() {
        Some("ping")
    } else {
        None
    };
    if remote.protocol == "tcp" {
        let tcp = network::tcping(&remote.ip.to_string(), remote.port, 2).await;
        if tcp.is_none() {
            return Ok((
                remote.clone(),
                json!({"ip":remote.ip,"port":remote.port,"protocol":remote.protocol,"endpoint_id":remote.endpoint_id,"index":index,"ok":false,"latency_ms":null,"error":"TCP 端口不可达"}),
            ));
        }
        if latency.is_none() {
            latency = tcp;
            method = Some("tcping")
        }
    }
    let path = run.join(format!("probe-{index}.conf"));
    let log = run.join(format!("probe-{index}.log"));
    let mut options = base;
    options.extend([
        format!(
            "remote {} {} {}",
            remote.ip,
            remote.port,
            if remote.protocol == "udp" {
                "udp4"
            } else {
                "tcp4-client"
            }
        ),
        "dev null".into(),
        "route-noexec".into(),
        "ifconfig-noexec".into(),
        "nobind".into(),
        "auth-nocache".into(),
        "script-security 1".into(),
        "verb 3".into(),
    ]);
    storage::private_write(&path, (options.join("\n") + "\n").as_bytes())?;
    let out = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&log)?;
    fs::set_permissions(&log, fs::Permissions::from_mode(0o600))?;
    let mut process = Command::new("timeout")
        .args(["--kill-after=2", "10", "openvpn", "--config"])
        .arg(&path)
        .stdout(out.try_clone()?)
        .stderr(out)
        .kill_on_drop(true)
        .spawn()?;
    let mut ok = false;
    loop {
        if fs::read_to_string(&log)?.contains("Initialization Sequence Completed") {
            ok = true;
            break;
        }
        if process.try_wait()?.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if process.try_wait()?.is_none() {
        // timeout 会把收到的 TERM 转发给 OpenVPN，避免遗留探测进程。
        network::output(
            "kill",
            &args(&["-TERM", &process.id().unwrap().to_string()]),
            None,
            3,
        )
        .await?;
        let _ = process.wait().await;
    }
    Ok((
        remote.clone(),
        json!({"ip":remote.ip,"port":remote.port,"protocol":remote.protocol,"endpoint_id":remote.endpoint_id,"index":index,"ok":ok,"latency_ms":latency,"method":method,"error":if ok{None}else{Some("OpenVPN 握手未成功，请检查端口和证书")} }),
    ))
}

async fn select_endpoint(
    rt: &Runtime,
    base: &[String],
    candidates: Vec<Remote>,
) -> Result<Vec<Remote>> {
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
    let mut tasks = JoinSet::new();
    for (index, remote) in candidates.into_iter().enumerate() {
        let run = rt.run.clone();
        let base = base.to_vec();
        let semaphore = semaphore.clone();
        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await?;
            probe_endpoint(run, base, remote, index).await
        });
    }
    let mut results = Vec::new();
    while let Some(item) = tasks.join_next().await {
        results.push(item??)
    }
    results.sort_by_key(|(_, value)| value["index"].as_u64());
    rt.json(
        "endpoint-probes.json",
        &results
            .iter()
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>(),
    )?;
    let available = results
        .iter()
        .filter(|(_, item)| item["ok"] == true)
        .collect::<Vec<_>>();
    let measured = available
        .iter()
        .copied()
        .filter(|(_, item)| item["latency_ms"].is_number())
        .min_by(|(_, a), (_, b)| {
            a["latency_ms"]
                .as_f64()
                .unwrap()
                .total_cmp(&b["latency_ms"].as_f64().unwrap())
        });
    let best = measured
        .or_else(|| available.first().copied())
        .ok_or("所有 endpoint 都未完成 OpenVPN 握手，请查看探测结果与日志")?;
    rt.json("selected-endpoint.json", &best.0)?;
    Ok(vec![best.0.clone()])
}

async fn render_http(rt: &Runtime, config: &Active) -> Result<()> {
    let proxy = &config.settings.http;
    let auth = if proxy.authentication == config::Authentication::Password {
        format!("BasicAuth {} {}", proxy.username, proxy.password)
    } else {
        String::new()
    };
    let file = rt.run.join("tinyproxy.conf");
    rt.write(&file,format!("User tinyproxy\nGroup tinyproxy\nPort {}\nListen 0.0.0.0\nTimeout 600\nMaxClients 128\nLogLevel Info\nLogFile \"/run/tinyproxy/log.pipe\"\nPidFile \"/run/tinyproxy/tinyproxy.pid\"\nDisableViaHeader Yes\n{auth}\n",HTTP_PORT).as_bytes())?;
    network::command(
        "chown",
        &["root:tinyproxy".into(), file.display().to_string()],
        None,
    )
    .await?;
    fs::set_permissions(file, fs::Permissions::from_mode(0o640))?;
    Ok(())
}

async fn passwd(name: &str) -> Result<Option<Vec<String>>> {
    let response = network::output("getent", &args(&["passwd", name]), None, 5).await?;
    Ok(if response.status.success() {
        Some(
            String::from_utf8_lossy(&response.stdout)
                .trim()
                .split(':')
                .map(str::to_owned)
                .collect(),
        )
    } else {
        None
    })
}
async fn managed_user(
    rt: &Runtime,
    config: &Active,
    name: &str,
    role: &str,
    password: &str,
) -> Result<(u32, u32, PathBuf)> {
    let marker = rt.data.join(".state").join(format!("{role}-user"));
    let previous = fs::read_to_string(&marker).ok();
    let profile = rt.data.join("profiles").join(&config.id);
    let uid_path = profile.join(".state").join(format!("{role}-uid"));
    let uid = if uid_path.exists() {
        fs::read_to_string(&uid_path)?.parse::<u32>()?
    } else {
        let next = rt.data.join(".state/next-uid");
        let mut uid = fs::read_to_string(&next)
            .ok()
            .map(|value| value.parse::<u32>())
            .transpose()?
            .unwrap_or(20000);
        while passwd(&uid.to_string()).await?.is_some() {
            uid += 1
        }
        rt.write(&uid_path, uid.to_string().as_bytes())?;
        rt.write(&next, (uid + 1).to_string().as_bytes())?;
        uid
    };
    let home = if role == "sshd" {
        profile.join("ssh/home")
    } else {
        PathBuf::from("/nonexistent")
    };
    let shell = if role == "sshd" {
        "/bin/bash"
    } else {
        "/usr/sbin/nologin"
    };
    if passwd(name).await?.is_some() {
        if previous.as_deref() != Some(name) {
            return Err(format!("账户 {name} 已被系统或其他服务占用").into());
        }
    } else {
        network::command(
            "useradd",
            &args(&[
                "--no-create-home",
                "--uid",
                &uid.to_string(),
                "--home-dir",
                &home.display().to_string(),
                "--shell",
                shell,
                name,
            ]),
            None,
        )
        .await?;
    }
    network::command(
        "usermod",
        &args(&[
            "--home",
            &home.display().to_string(),
            "--shell",
            shell,
            name,
        ]),
        None,
    )
    .await?;
    if let Some(previous) = previous
        && previous != name
        && passwd(&previous).await?.is_some()
    {
        network::command("userdel", &args(&[&previous]), None).await?;
    }
    rt.write(&marker, name.as_bytes())?;
    if password.is_empty() {
        network::command("usermod", &args(&["--password", "*", name]), None).await?;
    } else {
        network::command(
            "chpasswd",
            &[],
            Some(format!("{name}:{password}\n").as_bytes()),
        )
        .await?;
    }
    let account = passwd(name).await?.unwrap();
    Ok((account[2].parse()?, account[3].parse()?, home))
}

async fn render_socks(rt: &Runtime, config: &Active) -> Result<()> {
    let proxy = &config.settings.socks5;
    let auth = proxy.authentication == config::Authentication::Password;
    let method = if auth { "username" } else { "none" };
    let user = if auth {
        managed_user(rt, config, &proxy.username, "socks5", &proxy.password).await?;
        format!("user: {}", proxy.username)
    } else {
        String::new()
    };
    rt.write(&rt.run.join("danted.conf"),format!("logoutput: stderr\ninternal: 0.0.0.0 port = {}\nexternal: tun0\nclientmethod: none\nsocksmethod: {method}\nuser.privileged: root\nuser.unprivileged: nobody\nclient pass {{\n from: 0.0.0.0/0 to: 0.0.0.0/0\n}}\nsocks pass {{\n from: 0.0.0.0/0 to: 0.0.0.0/0\n command: connect\n socksmethod: {method}\n {user}\n log: error\n}}\n",SOCKS5_PORT).as_bytes())?;
    Ok(())
}

async fn render_ssh(rt: &Runtime, config: &Active) -> Result<()> {
    let ssh = &config.settings.sshd;
    let (uid, gid, home) = managed_user(rt, config, &ssh.username, "sshd", &ssh.password).await?;
    fs::create_dir_all(&home)?;
    fs::set_permissions(home.parent().unwrap(), fs::Permissions::from_mode(0o711))?;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
    std::os::unix::fs::chown(&home, Some(uid), Some(gid))?;
    let keys = home.join(".ssh/authorized_keys");
    rt.write(
        &keys,
        (ssh.authorized_keys.trim().to_owned() + "\n").as_bytes(),
    )?;
    for path in [&keys, keys.parent().unwrap()] {
        std::os::unix::fs::chown(path, Some(uid), Some(gid))?;
    }
    if !ssh.authorized_keys.trim().is_empty() {
        network::command(
            "ssh-keygen",
            &args(&["-l", "-f", &keys.display().to_string()]),
            None,
        )
        .await?;
    }
    let hostkey = home.parent().unwrap().join("ssh_host_ed25519_key");
    if !hostkey.exists() {
        network::command(
            "ssh-keygen",
            &args(&[
                "-t",
                "ed25519",
                "-N",
                "",
                "-f",
                &hostkey.display().to_string(),
            ]),
            None,
        )
        .await?;
    }
    std::os::unix::fs::chown(&hostkey, Some(0), Some(0))?;
    fs::set_permissions(&hostkey, fs::Permissions::from_mode(0o600))?;
    let sudoers = Path::new("/etc/sudoers.d/vpnweb");
    if ssh.sudo {
        rt.write(
            sudoers,
            format!("{} ALL=(ALL) NOPASSWD: ALL\n", ssh.username).as_bytes(),
        )?;
        fs::set_permissions(sudoers, fs::Permissions::from_mode(0o440))?;
    } else if sudoers.exists() {
        fs::remove_file(sudoers)?;
    }
    let path = rt.run.join("sshd_config");
    // MaxSessions=0 禁止终端、命令和 SFTP 会话，仍允许认证后的端口转发。
    let max_sessions = if ssh.allow_login { 10 } else { 0 };
    rt.write(&path,format!("Port {}\nListenAddress 0.0.0.0\nHostKey {}\nPidFile /run/sshd/vpnweb.pid\nPermitRootLogin no\nAllowUsers {}\nPasswordAuthentication {}\nPubkeyAuthentication yes\nAuthorizedKeysFile .ssh/authorized_keys\nKbdInteractiveAuthentication no\nUsePAM no\nMaxSessions {max_sessions}\nAllowTcpForwarding yes\nGatewayPorts clientspecified\nPermitTunnel yes\nPrintMotd no\nSubsystem sftp internal-sftp\nLogLevel VERBOSE\n",SSH_PORT,hostkey.display(),ssh.username,if ssh.password.is_empty(){"no"}else{"yes"}).as_bytes())?;
    network::command(
        "sshd",
        &args(&["-t", "-f", &path.display().to_string()]),
        None,
    )
    .await?;
    Ok(())
}

async fn isolate(rt: &Runtime, config: &Active) -> Result<()> {
    let previous = fs::read_to_string(rt.data.join(".state/active-profile")).unwrap_or_default();
    if previous != config.id {
        for role in ["socks5", "sshd"] {
            let path = rt.data.join(".state").join(format!("{role}-user"));
            if let Ok(name) = fs::read_to_string(&path) {
                if passwd(&name).await?.is_some() {
                    network::command("userdel", &args(&[&name]), None).await?;
                }
                fs::remove_file(path)?;
            }
        }
    }
    let profiles = rt.data.join("profiles");
    fs::create_dir_all(&profiles)?;
    fs::set_permissions(&profiles, fs::Permissions::from_mode(0o711))?;
    for path in fs::read_dir(&profiles)? {
        let path = path?.path();
        if path.is_dir() {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        }
    }
    let profile = profiles.join(&config.id);
    fs::create_dir_all(&profile)?;
    fs::set_permissions(profile, fs::Permissions::from_mode(0o711))?;
    rt.write(&rt.data.join(".state/active-profile"), config.id.as_bytes())?;
    Ok(())
}

async fn close_ssh_sessions(rt: &Runtime) -> Result<()> {
    let marker = rt.data.join(".state/sshd-user");
    if let Ok(name) = fs::read_to_string(marker)
        && let Some(account) = passwd(&name).await?
    {
        // OpenSSH 登录会话会创建独立进程组，停止主进程不能关闭它们。
        let result =
            network::output("pkill", &args(&["-KILL", "-u", &account[2]]), None, 5).await?;
        if !result.status.success() && result.status.code() != Some(1) {
            return Err("无法关闭旧配置的 SSH 会话".into());
        }
    }
    Ok(())
}

async fn apply(rt: &Runtime, service: &str, action: &str) -> Result<Value> {
    let saved = rt.saved()?;
    validate(&saved.settings)?;
    let mut active = rt.active()?;
    let mut endpoints = rt.endpoints()?;
    let targets = if service == "all" {
        vec!["openvpn", "http", "socks5", "sshd"]
    } else {
        vec![service]
    };
    if action == "start" {
        match service {
            "openvpn"
                if saved.settings.vpn.config.trim().is_empty()
                    && saved.settings.vpn.endpoints.is_empty() =>
            {
                return Err("请先填写 OpenVPN 配置".into());
            }
            "http" if !saved.settings.http.enabled => return Err("请先启用 HTTP 代理并保存".into()),
            "socks5" if !saved.settings.socks5.enabled => {
                return Err("请先启用 SOCKS5 并保存".into());
            }
            "sshd" if !saved.settings.sshd.enabled => return Err("请先启用 SSH 并保存".into()),
            _ => {}
        }
    }
    for name in &targets {
        stop(name).await?;
        if *name == "sshd" {
            close_ssh_sessions(rt).await?;
        }
    }
    if action == "stop" {
        return Ok(json!({"message":"服务已停止"}));
    }
    for name in &targets {
        match *name {
            "openvpn" => active.settings.vpn = saved.settings.vpn.clone(),
            "http" => active.settings.http = saved.settings.http.clone(),
            "socks5" => active.settings.socks5 = saved.settings.socks5.clone(),
            "sshd" => active.settings.sshd = saved.settings.sshd.clone(),
            _ => return Err("未知服务".into()),
        }
    }
    if service == "all" {
        active.id = saved.id.clone();
        active.name = saved.name.clone();
        active.settings.health = saved.settings.health.clone();
        isolate(rt, &active).await?;
    }
    rt.json("applied.json", &active)?;
    if targets.contains(&"openvpn") {
        firewall(rt, &active, &endpoints).await?;
        let rendered = render_vpn(rt, &active).await;
        match rendered {
            Ok(value) => endpoints = value,
            Err(error) => {
                firewall(rt, &active, &[]).await?;
                return Err(error);
            }
        }
        rt.json("endpoints.json", &endpoints)?;
        write_dns(rt, &active, Some(vec![]))?;
    }
    if targets.contains(&"http") && active.settings.http.enabled {
        render_http(rt, &active).await?
    }
    if targets.contains(&"socks5") && active.settings.socks5.enabled {
        render_socks(rt, &active).await?
    }
    if targets.contains(&"sshd") && active.settings.sshd.enabled {
        render_ssh(rt, &active).await?
    }
    firewall(rt, &active, &endpoints).await?;
    rt.json("applied.json", &active)?;
    for name in targets {
        let enabled = match name {
            "openvpn" => {
                (!active.settings.vpn.config.trim().is_empty()
                    || !active.settings.vpn.endpoints.is_empty())
                    && (action != "startup" || active.settings.vpn.autoconnect)
            }
            "http" => active.settings.http.enabled,
            "socks5" => active.settings.socks5.enabled,
            "sshd" => active.settings.sshd.enabled,
            _ => false,
        };
        if enabled {
            supervisor("start", Some(name)).await?;
        }
    }
    Ok(json!({"message":"配置已应用"}))
}

fn process_tree(pid: u32) -> HashSet<u32> {
    let mut pids = HashSet::new();
    let mut pending = vec![pid];
    while let Some(pid) = pending.pop() {
        if !pids.insert(pid) {
            continue;
        }
        // Dante 由 socks-run 启动，Tinyproxy 也会派生 worker；监听者可能是子进程。
        if let Ok(children) = fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")) {
            pending.extend(
                children
                    .split_whitespace()
                    .filter_map(|pid| pid.parse::<u32>().ok()),
            );
        }
    }
    pids
}

fn listening_port(listing: &str, pids: &HashSet<u32>, expected: u16) -> Option<u16> {
    let port = |line: &str| -> Option<u16> {
        line.split_whitespace()
            .nth(3)?
            .rsplit(':')
            .next()?
            .parse()
            .ok()
    };
    let actual = listing
        .lines()
        .filter(|line| {
            line.split("pid=").skip(1).any(|process| {
                process
                    .split(',')
                    .next()
                    .and_then(|pid| pid.parse::<u32>().ok())
                    .is_some_and(|pid| pids.contains(&pid))
            })
        })
        .filter_map(port)
        .min();
    // Tinyproxy 降权后，缺少 SYS_PTRACE 的容器无法读取其 socket PID。
    // 仅对未公开进程信息的监听项，按本程序生成的固定服务端口检测。
    actual.or_else(|| {
        listing
            .lines()
            .find(|line| !line.contains("users:") && port(line) == Some(expected))
            .map(|_| expected)
    })
}

async fn status(rt: &Runtime) -> Result<Value> {
    let active = rt.active()?;
    let (services, firewall) = tokio::try_join!(processes(), health::firewall_ready())?;
    let container_health = health::container_status(&services, firewall);
    let vpn = vpn_state(rt).await;
    let vpn_health = health::vpn_status(rt, &vpn).await?;
    let routes: Value = serde_json::from_str(
        &network::command(
            "ip",
            &args(&["-j", "-4", "route", "show", "table", "main"]),
            None,
        )
        .await?,
    )?;
    let listing = network::command("ss", &args(&["-H", "-lntp"]), None).await?;
    let ports = [
        ("backend", WEB_PORT, "PUBLISHED_WEB_PORT"),
        ("http", HTTP_PORT, "PUBLISHED_HTTP_PORT"),
        ("socks5", SOCKS5_PORT, "PUBLISHED_SOCKS5_PORT"),
        ("sshd", SSH_PORT, "PUBLISHED_SSH_PORT"),
    ]
    .into_iter()
    .map(|(name, target_port, published_env)| {
        let pid = services
            .iter()
            .find(|service| service["name"] == name)
            .and_then(|service| service["pid"].as_u64())
            .unwrap_or(0) as u32;
        let actual = (pid != 0)
            .then(|| listening_port(&listing, &process_tree(pid), target_port))
            .flatten();
        let published = std::env::var(published_env)
            .ok()
            .and_then(|port| port.parse::<u16>().ok())
            .filter(|port| *port != 0);
        json!({"name":name,"port":actual.unwrap_or(target_port),"listening":actual.is_some(),"published_port":published,"target_port":target_port})
    })
    .collect::<Vec<_>>();
    Ok(
        json!({"services":services,"vpn":vpn,"kill_switch":firewall,"ports":ports,"routes":routes,"dns":rt.read("dns.json",json!({"mode":active.settings.vpn.dns_mode,"pushed":[],"effective":[],"source":"none"}))?,"active_profile":active.id,"httpdns":rt.read("httpdns.json",json!({"state":"probing","results":[]}))?,"httpdns_presets":network::providers(),"endpoint_probes":rt.read("endpoint-probes.json",json!([]))?,"selected_endpoint":rt.read("selected-endpoint.json",Value::Null)?,"remote_endpoints":rt.endpoints()?,"health":{"container":container_health,"vpn":vpn_health}}),
    )
}

async fn socks_run(rt: &Runtime) -> Result<Value> {
    loop {
        let addresses = network::output(
            "ip",
            &args(&["-j", "-4", "address", "show", "dev", "tun0"]),
            None,
            5,
        )
        .await?;
        let address = if addresses.status.success() {
            let value: Value = serde_json::from_slice(&addresses.stdout)?;
            value[0]["addr_info"]
                .as_array()
                .and_then(|items| items.iter().find(|item| item["family"] == "inet"))
                .and_then(|item| item["local"].as_str())
                .map(str::to_owned)
        } else {
            None
        };
        if address.is_none() {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }
        let mut child = Command::new("danted")
            .args(["-f", "/run/vpnweb/danted.conf"])
            .kill_on_drop(true)
            .spawn()?;
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if let Some(exit) = child.try_wait()? {
                if !exit.success() {
                    return Err(format!("Dante 已退出：{exit}").into());
                }
                break;
            }
            let current = network::output(
                "ip",
                &args(&["-j", "-4", "address", "show", "dev", "tun0"]),
                None,
                5,
            )
            .await?;
            if !current.status.success()
                || !String::from_utf8_lossy(&current.stdout).contains(address.as_ref().unwrap())
            {
                child.kill().await?;
                break;
            }
        }
        let _ = rt;
    }
}

pub async fn dispatch(data: PathBuf, values: &[String]) -> Result<Value> {
    let rt = Runtime {
        data,
        run: std::env::var("RUN_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| "/run/vpnweb".into()),
    };
    let operation = values.first().map(String::as_str).ok_or("缺少运行时操作")?;
    let _lock = if matches!(operation, "apply" | "bootstrap" | "dns") {
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(rt.run.join("apply.lock"))?;
        file.lock()?;
        Some(file)
    } else {
        None
    };
    match operation {
        "validate" => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            validate(&serde_json::from_str(&text)?)?;
            Ok(json!({"message":"配置有效"}))
        }
        "init" => {
            let saved = rt.saved()?;
            firewall(&rt, &saved, &[]).await?;
            Ok(json!({"message":"出口防火墙已启用"}))
        }
        "bootstrap" => {
            for _ in 0..100 {
                if supervisor("status", None).await.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            apply(&rt, "all", "startup").await
        }
        "apply" => {
            apply(
                &rt,
                values.get(1).ok_or("缺少服务")?,
                values.get(2).ok_or("缺少操作")?,
            )
            .await
        }
        "status" => status(&rt).await,
        "dns" => {
            let mut active = rt.active()?;
            let saved = rt.saved()?;
            active.settings.vpn.dns = saved.settings.vpn.dns;
            active.settings.vpn.dns_mode = saved.settings.vpn.dns_mode;
            rt.json("applied.json", &active)?;
            write_dns(&rt, &active, None)?;
            Ok(json!({"message":"DNS 已热更新，VPN 和代理无需重启"}))
        }
        "vpn-up" => {
            let active = rt.active()?;
            let mut pushed = Vec::new();
            for (name, value) in std::env::vars() {
                if name.starts_with("foreign_option_") {
                    let fields = value.split_whitespace().collect::<Vec<_>>();
                    if fields.len() == 3 && fields[..2] == ["dhcp-option", "DNS"] {
                        let ip = fields[2].parse::<std::net::IpAddr>()?.to_string();
                        if !pushed.contains(&ip) {
                            pushed.push(ip)
                        }
                    }
                }
            }
            write_dns(&rt, &active, Some(pushed))?;
            let path = rt.data.join(".state/last-endpoints.yaml");
            let mut previous = if path.exists() {
                serde_saphyr::from_str::<HashMap<String, String>>(&fs::read_to_string(&path)?)?
            } else {
                HashMap::new()
            };
            let selected = rt.read("selected-endpoint.json", json!({}))?;
            previous.insert(
                active.id,
                selected["endpoint_id"].as_str().unwrap_or("").into(),
            );
            rt.write(&path, serde_saphyr::to_string(&previous)?.as_bytes())?;
            Ok(json!({"message":"已探测并应用 OpenVPN 下发的 DNS"}))
        }
        "socks-run" => socks_run(&rt).await,
        "httpdns-probe" => {
            rt.json("httpdns.json", &json!({"state":"probing","results":[]}))?;
            let results = network::httpdns_all(
                "example.com",
                Some(network::configured(&rt.saved()?.settings.vpn)),
                &[network::RecordType::A],
            )
            .await?;
            let value =
                json!({"state":"ready","results":results,"checked_at":network::timestamp()});
            rt.json("httpdns.json", &value)?;
            Ok(value)
        }
        "dns-test" => {
            let domain = network::domain(values.get(1).ok_or("请输入域名")?)?;
            let config = rt.saved()?;
            let dns = rt.read("dns.json", json!({}))?;
            // 仅测试已应用的地址，避免未启用或尚未应用的配置混入当前结果。
            let mut servers = dns["effective"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            servers.sort();
            servers.dedup();
            let mut tasks = JoinSet::new();
            for server in servers {
                for record_type in [network::RecordType::A, network::RecordType::Aaaa] {
                    let domain = domain.clone();
                    let server = server.clone();
                    tasks.spawn(
                        async move { network::dns(Some(&server), &domain, record_type).await },
                    );
                }
            }
            let (default_a, default_aaaa, http) = tokio::join!(
                network::dns(None, &domain, network::RecordType::A),
                network::dns(None, &domain, network::RecordType::Aaaa),
                network::httpdns_all(
                    &domain,
                    Some(network::configured(&config.settings.vpn)),
                    &[network::RecordType::A, network::RecordType::Aaaa]
                )
            );
            let http = http?;
            let mut vpn = Vec::new();
            while let Some(result) = tasks.join_next().await {
                vpn.push(result?)
            }
            Ok(
                json!({"domain":domain,"default_dns":[default_a,default_aaaa],"vpn_dns":vpn,"httpdns":http}),
            )
        }
        "container-health" => {
            let (services, firewall) = tokio::try_join!(processes(), health::firewall_ready())?;
            Ok(health::container_status(&services, firewall))
        }
        "health" => health::check(&rt).await,
        "health-worker" => health::worker(&rt).await,
        _ => Err("未知运行时操作".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_service_ports_by_process_instead_of_expected_port() {
        let listing = "LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:((\"other\",pid=142,fd=3))\n\
                       LISTEN 0 128 [::]:2222 [::]:* users:((\"sshd\",pid=42,fd=4))";
        assert_eq!(
            listening_port(listing, &HashSet::from([42]), 22),
            Some(2222)
        );
        assert_eq!(listening_port(listing, &HashSet::from([99]), 22), None);
    }

    #[test]
    fn detects_a_socket_shared_by_service_workers() {
        let listing = "LISTEN 0 128 0.0.0.0:1080 0.0.0.0:* users:((\"danted\",pid=51,fd=7),(\"danted\",pid=52,fd=7))";
        assert_eq!(
            listening_port(listing, &HashSet::from([50, 52]), 1080),
            Some(1080)
        );
    }

    #[test]
    fn detects_listeners_when_the_kernel_hides_socket_owners() {
        let listing = "LISTEN 0 1024 0.0.0.0:8888 0.0.0.0:*";
        assert_eq!(
            listening_port(listing, &HashSet::from([154]), 8888),
            Some(8888)
        );
        assert_eq!(listening_port(listing, &HashSet::from([154]), 1080), None);
    }

    #[test]
    fn parses_client_options_and_blocks_external_programs() {
        let mut vpn = config::Vpn {
            config:
                "client\ndev tun\nremote vpn.example.com 1194 udp\nremote 192.0.2.1 443 tcp-client"
                    .into(),
            ..Default::default()
        };
        let (kept, remotes) = parse(&vpn).unwrap();
        assert_eq!(remotes.len(), 2);
        assert!(!kept.iter().any(|line| line == "dev tun"));
        vpn.config += "\nup /tmp/run";
        assert!(parse(&vpn).is_err());
    }

    #[test]
    fn all_routes_override_original_limits_without_removing_dns_filters() {
        let vpn = config::Vpn {
            config: "client\nremote 192.0.2.1\nroute-nopull\nroute 10.0.0.0 255.0.0.0 net_gateway\nroute-metric 800\nredirect-gateway local\npull-filter ignore \"route \"\npull-filter reject \"redirect-gateway\"\npull-filter ignore \"\"\npull-filter ignore \"dhcp-option DNS \"".into(),
            ..Default::default()
        };
        let (options, _) = parse(&vpn).unwrap();
        assert!(options.contains(&"redirect-gateway def1".into()));
        assert!(!options.iter().any(|line| line.starts_with("route")
            || line == "redirect-gateway local"
            || line.starts_with("pull-filter reject")
            || line == "pull-filter ignore \"\""));
        assert!(options.contains(&"pull-filter ignore \"route \"".into()));
        assert!(options.contains(&"pull-filter ignore \"dhcp-option DNS \"".into()));
        assert_eq!(options[0], "pull-filter accept \"dhcp-option DNS \"");
    }

    #[test]
    fn configured_routes_preserve_limits_and_do_not_force_a_default_route() {
        let routing = [
            "route-nopull",
            "route 10.0.0.0 255.0.0.0",
            "route-metric 800",
            "pull-filter ignore \"route \"",
        ];
        let mut vpn = config::Vpn {
            config: format!("client\nremote 192.0.2.1\n{}", routing.join("\n")),
            route_mode: RouteMode::Config,
            ..Default::default()
        };
        let (options, _) = parse(&vpn).unwrap();
        assert!(
            routing
                .iter()
                .all(|line| options.iter().any(|option| option == line))
        );
        assert!(
            !options
                .iter()
                .any(|line| line.starts_with("redirect-gateway"))
        );
        vpn.config += "\nredirect-gateway def1 bypass-dhcp";
        assert!(
            parse(&vpn)
                .unwrap()
                .0
                .contains(&"redirect-gateway def1 bypass-dhcp".into())
        );
        vpn.config += "\nup /tmp/run";
        assert!(parse(&vpn).is_err());
    }
    #[test]
    fn rejects_missing_material_and_preserves_inline_direction() {
        let mut vpn = config::Vpn {
            config: "remote 192.0.2.1\nca ca.crt".into(),
            ..Default::default()
        };
        assert!(parse(&vpn).is_err());
        vpn.config = "remote 192.0.2.1\n<tls-auth>\nkey\n</tls-auth>\nkey-direction 0".into();
        assert!(parse(&vpn).unwrap().0.contains(&"key-direction 0".into()));
    }
}
