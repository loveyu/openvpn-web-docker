use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    net::Ipv4Addr,
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::{io::AsyncWriteExt, process::Command, task::JoinSet};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub region: String,
    pub url: String,
    pub ips: Vec<Ipv4Addr>,
    pub protocol: String,
}

pub fn providers() -> Vec<Provider> {
    [
        (
            "tencent",
            "腾讯 DNSPod",
            "国内",
            "https://doh.pub/dns-query",
            ["1.12.12.12", "120.53.53.53"],
        ),
        (
            "aliyun",
            "阿里 AliDNS",
            "国内",
            "https://dns.alidns.com/dns-query",
            ["223.5.5.5", "223.6.6.6"],
        ),
        (
            "cloudflare",
            "Cloudflare",
            "全球",
            "https://cloudflare-dns.com/dns-query",
            ["1.1.1.1", "1.0.0.1"],
        ),
        (
            "google",
            "Google Public DNS",
            "全球",
            "https://dns.google/dns-query",
            ["8.8.8.8", "8.8.4.4"],
        ),
        (
            "quad9",
            "Quad9",
            "全球",
            "https://dns.quad9.net/dns-query",
            ["9.9.9.9", "149.112.112.112"],
        ),
    ]
    .into_iter()
    .map(|(id, name, region, url, ips)| Provider {
        id: id.into(),
        name: name.into(),
        region: region.into(),
        url: url.into(),
        ips: ips.map(|ip| ip.parse().unwrap()).into(),
        protocol: "doh".into(),
    })
    .collect()
}

pub fn configured(vpn: &crate::config::Vpn) -> Provider {
    Provider {
        id: "custom".into(),
        name: "当前首选 HTTPDNS".into(),
        region: "自定义".into(),
        url: vpn.httpdns_url.clone(),
        ips: vpn.httpdns_ips.clone(),
        protocol: match vpn.httpdns_protocol {
            crate::config::HttpDnsProtocol::Doh => "doh",
            crate::config::HttpDnsProtocol::Json => "json",
        }
        .into(),
    }
}

pub fn domain(value: &str) -> Result<String> {
    let value = value.trim().trim_end_matches('.');
    if value.is_empty()
        || value.len() > 253
        || value.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return Err("请输入有效的域名或 IPv4 地址；国际化域名请使用 Punycode".into());
    }
    Ok(value.into())
}

pub fn http_target(url: &str) -> Result<(String, u16)> {
    let uri: axum::http::Uri = url.parse()?;
    if !matches!(uri.scheme_str(), Some("http" | "https"))
        || uri.authority().is_none_or(|a| a.as_str().contains('@'))
        || url.contains('#')
    {
        return Err("HTTPDNS/HTTP URL 必须是无认证信息的 HTTP 或 HTTPS 地址".into());
    }
    Ok((
        uri.host().ok_or("URL 缺少主机")?.into(),
        uri.port_u16()
            .unwrap_or(if uri.scheme_str() == Some("https") {
                443
            } else {
                80
            }),
    ))
}

pub async fn output(
    program: &str,
    args: &[String],
    input: Option<&[u8]>,
    timeout: u64,
) -> Result<std::process::Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    if let Some(input) = input {
        child.stdin.take().unwrap().write_all(input).await?;
    }
    Ok(tokio::time::timeout(Duration::from_secs(timeout), child.wait_with_output()).await??)
}

pub async fn command(program: &str, args: &[String], input: Option<&[u8]>) -> Result<String> {
    let result = output(program, args, input, 30).await?;
    if !result.status.success() {
        return Err(format!(
            "{program} 执行失败：{}",
            String::from_utf8_lossy(&result.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&result.stdout).into_owned())
}

pub fn question(name: &str) -> Result<Vec<u8>> {
    let mut bytes = vec![0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in domain(name)?.split('.') {
        bytes.push(label.len() as u8);
        bytes.extend_from_slice(label.as_bytes());
    }
    bytes.extend_from_slice(&[0, 0, 1, 0, 1]);
    Ok(bytes)
}

pub fn answers(bytes: &[u8]) -> Result<Vec<Ipv4Addr>> {
    fn word(b: &[u8], p: usize) -> Result<u16> {
        Ok(u16::from_be_bytes(
            b.get(p..p + 2).ok_or("DNS 响应不完整")?.try_into()?,
        ))
    }
    fn skip(b: &[u8], mut p: usize) -> Result<usize> {
        loop {
            let size = *b.get(p).ok_or("DNS 名称不完整")?;
            if size & 0xc0 == 0xc0 {
                if p + 2 > b.len() {
                    return Err("DNS 压缩指针不完整".into());
                }
                return Ok(p + 2);
            }
            if size > 63 {
                return Err("DNS 名称无效".into());
            }
            p += 1;
            if size == 0 {
                return Ok(p);
            }
            p += usize::from(size);
        }
    }
    let flags = word(bytes, 2)?;
    if flags & 0x8000 == 0 || flags & 0xf != 0 {
        return Err(format!("DNS 响应错误 RCODE={}", flags & 0xf).into());
    }
    let mut p = 12;
    for _ in 0..word(bytes, 4)? {
        p = skip(bytes, p)? + 4;
    }
    let mut result = Vec::new();
    for _ in 0..word(bytes, 6)? {
        p = skip(bytes, p)?;
        let kind = word(bytes, p)?;
        let class = word(bytes, p + 2)?;
        let length = usize::from(word(bytes, p + 8)?);
        p += 10;
        let data = bytes.get(p..p + length).ok_or("DNS 数据不完整")?;
        if kind == 1 && class == 1 && length == 4 {
            let ip = Ipv4Addr::new(data[0], data[1], data[2], data[3]);
            if !result.contains(&ip) {
                result.push(ip)
            }
        }
        p += length;
    }
    Ok(result)
}

pub async fn httpdns(provider: Provider, ip: Ipv4Addr, name: String) -> Value {
    let start = Instant::now();
    let mut result = json!({"provider":provider.name,"provider_id":provider.id,"region":provider.region,"url":provider.url,"protocol":provider.protocol,"ip":ip,"ok":false,"answers":[]});
    let work = async {
        let (host, port) = http_target(&provider.url)?;
        let mut args = vec![
            "--silent".into(),
            "--show-error".into(),
            "--fail".into(),
            "--ipv4".into(),
            "--noproxy".into(),
            "*".into(),
            "--connect-timeout".into(),
            "2".into(),
            "--max-time".into(),
            "4".into(),
            "--resolve".into(),
            format!("{host}:{port}:{ip}"),
        ];
        let query = question(&name)?;
        if provider.protocol == "doh" {
            args.extend([
                "--header".into(),
                "Content-Type: application/dns-message".into(),
                "--header".into(),
                "Accept: application/dns-message".into(),
                "--data-binary".into(),
                "@-".into(),
                "--url".into(),
                provider.url.clone(),
            ]);
        } else {
            args.extend([
                "--header".into(),
                "Accept: application/dns-json".into(),
                "--url".into(),
                format!(
                    "{}{}name={}&type=A",
                    provider.url,
                    if provider.url.contains('?') { "&" } else { "?" },
                    domain(&name)?
                ),
            ]);
        }
        let response = output(
            "curl",
            &args,
            if provider.protocol == "doh" {
                Some(&query)
            } else {
                None
            },
            5,
        )
        .await?;
        if !response.status.success() {
            return Err(String::from_utf8_lossy(&response.stderr)
                .trim()
                .to_owned()
                .into());
        }
        let records = if provider.protocol == "doh" {
            answers(&response.stdout)?
        } else {
            let value: Value = serde_json::from_slice(&response.stdout)?;
            if value["Status"] != 0 {
                return Err("HTTPDNS 返回错误状态".into());
            }
            value["Answer"]
                .as_array()
                .ok_or("HTTPDNS 未返回 A 记录")?
                .iter()
                .filter(|item| item["type"] == 1)
                .map(|item| {
                    item["data"]
                        .as_str()
                        .ok_or("无效 DNS 记录")?
                        .parse()
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
                })
                .collect::<Result<Vec<Ipv4Addr>>>()?
        };
        if records.is_empty() {
            return Err("未返回 IPv4 A 记录".into());
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(records)
    }
    .await;
    match work {
        Ok(records) => {
            result["ok"] = json!(true);
            result["answers"] = json!(records);
        }
        Err(error) => result["error"] = json!(error.to_string()),
    }
    result["latency_ms"] = json!((start.elapsed().as_secs_f64() * 100000.0).round() / 100.0);
    result
}

pub async fn httpdns_all(name: &str, custom: Option<Provider>) -> Result<Vec<Value>> {
    domain(name)?;
    let mut list = providers();
    if let Some(custom) = custom
        && !list
            .iter()
            .any(|item| item.url == custom.url && item.ips == custom.ips)
    {
        list.push(custom)
    }
    let mut tasks = JoinSet::new();
    for provider in list {
        for ip in provider.ips.clone() {
            tasks.spawn(httpdns(provider.clone(), ip, name.into()));
        }
    }
    let mut results = Vec::new();
    while let Some(item) = tasks.join_next().await {
        results.push(item?)
    }
    results.sort_by(|a, b| {
        (!a["ok"].as_bool().unwrap_or(false))
            .cmp(&(!b["ok"].as_bool().unwrap_or(false)))
            .then_with(|| {
                a["latency_ms"]
                    .as_f64()
                    .unwrap_or(f64::MAX)
                    .total_cmp(&b["latency_ms"].as_f64().unwrap_or(f64::MAX))
            })
    });
    Ok(results)
}

pub async fn resolve(vpn: &crate::config::Vpn, name: &str, catalog: &[Value]) -> Result<Value> {
    let preferred = configured(vpn);
    let mut tasks = JoinSet::new();
    for ip in &preferred.ips {
        tasks.spawn(httpdns(preferred.clone(), *ip, name.into()));
    }
    let mut results = Vec::new();
    while let Some(item) = tasks.join_next().await {
        let item = item?;
        if item["ok"] == true {
            results.push(item)
        }
    }
    results.sort_by(|a, b| {
        a["latency_ms"]
            .as_f64()
            .unwrap()
            .total_cmp(&b["latency_ms"].as_f64().unwrap())
    });
    if let Some(item) = results.first() {
        return Ok(item.clone());
    }
    for item in catalog.iter().filter(|item| item["ok"] == true) {
        let provider = Provider {
            id: item["provider_id"].as_str().unwrap_or("fallback").into(),
            name: item["provider"].as_str().unwrap_or("备用 HTTPDNS").into(),
            region: String::new(),
            url: item["url"].as_str().unwrap().into(),
            ips: vec![],
            protocol: item["protocol"].as_str().unwrap().into(),
        };
        let answer = httpdns(provider, item["ip"].as_str().unwrap().parse()?, name.into()).await;
        if answer["ok"] == true {
            return Ok(answer);
        }
    }
    httpdns_all(name, None)
        .await?
        .into_iter()
        .find(|item| item["ok"] == true)
        .ok_or_else(|| format!("全部 HTTPDNS 均无法解析 {name}，不会回退到系统 DNS").into())
}

pub async fn ping(host: &str, timeout: u64, count: u8) -> Option<f64> {
    let result = output(
        "ping",
        &[
            "-n".into(),
            "-c".into(),
            count.to_string(),
            "-W".into(),
            timeout.to_string(),
            host.into(),
        ],
        None,
        timeout * u64::from(count) + 2,
    )
    .await
    .ok()?;
    if !result.status.success() {
        return None;
    }
    String::from_utf8_lossy(&result.stdout)
        .lines()
        .last()?
        .split('=')
        .nth(1)?
        .trim()
        .split('/')
        .nth(1)?
        .parse()
        .ok()
}

pub async fn tcping(host: &str, port: u16, timeout: u64) -> Option<f64> {
    let start = Instant::now();
    tokio::time::timeout(
        Duration::from_secs(timeout),
        tokio::net::TcpStream::connect((host, port)),
    )
    .await
    .ok()?
    .ok()?;
    Some(start.elapsed().as_secs_f64() * 1000.0)
}

pub async fn vpn_dns(server: &str, name: &str) -> Value {
    let start = Instant::now();
    let mut result = json!({"provider":"VPN DNS","ip":server,"answers":[],"ok":false});
    let response = output(
        "dig",
        &[
            format!("@{server}"),
            name.into(),
            "A".into(),
            "+short".into(),
            "+time=2".into(),
            "+tries=1".into(),
        ],
        None,
        3,
    )
    .await;
    if let Ok(response) = response {
        let addresses = String::from_utf8_lossy(&response.stdout)
            .lines()
            .filter_map(|line| line.parse::<Ipv4Addr>().ok())
            .collect::<Vec<_>>();
        if response.status.success() && !addresses.is_empty() {
            result["ok"] = json!(true);
            result["answers"] = json!(addresses)
        }
    }
    if result["ok"] != true {
        result["error"] = json!("DNS 无响应或未返回 A 记录")
    }
    result["latency_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
    result
}

pub async fn custom_health(settings: &crate::config::Health) -> Value {
    use crate::config::CheckKind;
    let start = Instant::now();
    let mut result = json!({"enabled":settings.enabled,"kind":settings.kind,"ok":false});
    match settings.kind {
        CheckKind::Ping => {
            let response = output(
                "ping",
                &[
                    "-4".into(),
                    "-n".into(),
                    "-I".into(),
                    "tun0".into(),
                    "-c".into(),
                    settings.ping_count.to_string(),
                    "-w".into(),
                    settings.timeout_secs.to_string(),
                    "-W".into(),
                    settings.timeout_secs.to_string(),
                    settings.target.clone(),
                ],
                None,
                settings.timeout_secs + 1,
            )
            .await;
            result["ok"] = json!(response.is_ok_and(|response| response.status.success()));
            result["target"] = json!(settings.target);
        }
        CheckKind::Tcp => {
            let connect = async {
                let addresses =
                    tokio::net::lookup_host((settings.target.as_str(), settings.port)).await?;
                for address in addresses.filter(|address| address.is_ipv4()) {
                    let socket = tokio::net::TcpSocket::new_v4()?;
                    socket.bind_device(Some(b"tun0"))?;
                    if socket.connect(address).await.is_ok() {
                        return Ok::<_, std::io::Error>(());
                    }
                }
                Err(std::io::Error::other("VPN 隧道内 TCP 连接失败"))
            };
            result["ok"] = json!(
                tokio::time::timeout(Duration::from_secs(settings.timeout_secs), connect,)
                    .await
                    .is_ok_and(|value| value.is_ok())
            );
            result["target"] = json!(format!("{}:{}", settings.target, settings.port));
        }
        CheckKind::Http => {
            let mut arguments = vec![
                "--silent".into(),
                "--show-error".into(),
                "--noproxy".into(),
                "*".into(),
                "--ipv4".into(),
                "--interface".into(),
                "if!tun0".into(),
                "--max-time".into(),
                settings.timeout_secs.to_string(),
                "--output".into(),
                "/dev/null".into(),
                "--write-out".into(),
                "%{http_code}".into(),
                "--request".into(),
                match settings.http_method {
                    crate::config::HttpMethod::Get => "GET",
                    crate::config::HttpMethod::Head => "HEAD",
                }
                .into(),
                "--url".into(),
                settings.url.clone(),
            ];
            if matches!(settings.http_method, crate::config::HttpMethod::Head) {
                arguments.push("--head".into());
            }
            let response = output("curl", &arguments, None, settings.timeout_secs + 1).await;
            if let Ok(response) = response {
                let code = String::from_utf8_lossy(&response.stdout)
                    .parse::<u16>()
                    .unwrap_or(0);
                result["ok"] = json!(response.status.success() && code == settings.expected_status);
                result["http_status"] = json!(code);
                if !response.status.success() {
                    result["error"] = json!(String::from_utf8_lossy(&response.stderr).trim());
                }
            }
            result["target"] = json!(settings.url);
        }
    }
    if result["ok"] != true && result.get("error").is_none() {
        result["error"] = json!("VPN 隧道内检查未成功或结果不符合预期")
    }
    result["latency_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
    result["checked_at"] = json!(timestamp());
    result
}

pub fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_compressed_dns_answers_and_rejects_truncation() {
        let mut query = question("example.com").unwrap();
        query[2] = 0x81;
        query[3] = 0x80;
        query[7] = 1;
        query.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 30, 0, 4, 192, 0, 2, 1]);
        assert_eq!(answers(&query).unwrap(), vec![Ipv4Addr::new(192, 0, 2, 1)]);
        assert!(answers(&query[..query.len() - 2]).is_err());
    }
    #[test]
    fn rejects_invalid_domains_and_authenticated_urls() {
        assert!(domain("example.com;whoami").is_err());
        assert!(http_target("https://user:pass@example.com/dns-query").is_err());
        assert_eq!(
            http_target("https://doh.pub/dns-query").unwrap(),
            ("doh.pub".into(), 443)
        );
    }
}
