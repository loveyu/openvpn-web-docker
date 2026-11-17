use crate::network;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{net::Ipv4Addr, time::Instant};

pub mod path_trace;

async fn address(host: &str) -> network::Result<Ipv4Addr> {
    if let Ok(ip) = host.parse() {
        return Ok(ip);
    }
    // 网络工具使用当前默认 DNS，确保内网域名遵循 VPN 已应用的解析设置。
    let answer = network::dns(None, host, network::RecordType::A).await;
    if answer["ok"] != true {
        return Err(answer["error"].as_str().unwrap().into());
    }
    answer["answers"][0]
        .as_str()
        .unwrap()
        .parse()
        .map_err(Into::into)
}

fn failure(target: &str, start: Instant, error: impl std::fmt::Display) -> Value {
    json!({"target":target,"ok":false,"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
        "stdout":"","stderr":error.to_string(),"exit_code":null})
}

fn trace(bytes: &[u8]) -> String {
    const LIMIT: usize = 64 * 1024;
    if bytes.len() <= LIMIT {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    format!(
        "{}\n…中间日志已省略…\n{}",
        String::from_utf8_lossy(&bytes[..LIMIT / 2]),
        String::from_utf8_lossy(&bytes[bytes.len() - LIMIT / 2..])
    )
}

pub async fn tcping(host: &str, port: u16) -> Value {
    let target = format!("{host}:{port}");
    let start = Instant::now();
    let ip = match address(host).await {
        Ok(ip) => ip,
        Err(error) => return failure(&target, start, error),
    };
    let connection = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::TcpStream::connect((ip, port)),
    )
    .await;
    let error = match connection {
        Ok(Ok(_)) => None,
        Ok(Err(error)) => Some(error.to_string()),
        Err(_) => Some("TCP 连接超时（5 秒）".into()),
    };
    json!({"target":target,"ip":ip,"ok":error.is_none(),
        "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"exit_code":null,
        "stdout":if error.is_none(){format!("已连接 {ip}:{port}")}else{String::new()},
        "stderr":error.unwrap_or_default()})
}

pub async fn curl(url: &str, method: &str) -> Value {
    let start = Instant::now();
    let (host, port) = match network::http_target(url) {
        Ok(target) => target,
        Err(error) => return failure(url, start, error),
    };
    let ip = match address(&host).await {
        Ok(ip) => ip,
        Err(error) => return failure(url, start, error),
    };
    // 响应头和元数据放在唯一标记之后，响应正文按原始字节保留，图片不会经过文本解码。
    let marker = format!("\nVPNWEB_{}_INFO:", uuid::Uuid::new_v4());
    let mut args = [
        "--silent",
        "--show-error",
        "-vvv",
        "--noproxy",
        "*",
        "--proto",
        "=http,https",
        "--connect-timeout",
        "5",
        "--max-time",
        "20",
        "--max-filesize",
        "2097152",
    ]
    .map(str::to_owned)
    .to_vec();
    args.extend([
        "--resolve".into(),
        format!("{host}:{port}:{ip}"),
        "--write-out".into(),
        format!("{marker}{{\"info\":%{{json}},\"headers\":%{{header_json}}}}"),
    ]);
    if method == "HEAD" {
        args.push("--head".into());
    }
    args.extend(["--url".into(), url.into()]);
    let output = match network::output("curl", &args, None, 22).await {
        Ok(output) => output,
        Err(error) => return failure(url, start, error),
    };
    let (body, metadata) = split_curl_output(&output.stdout, marker.as_bytes());
    let info = &metadata["info"];
    let content_type = info["content_type"].as_str().unwrap_or("");
    let image = method == "GET"
        && content_type.to_ascii_lowercase().starts_with("image/")
        && !body.is_empty();
    json!({"target":url,"ip":ip,"method":method,"ok":output.status.success(),
        "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"exit_code":output.status.code(),
        "status_code":info["http_code"],"content_type":content_type,
        "response_headers":metadata["headers"],
        "stdout":if image{String::new()}else{String::from_utf8_lossy(body).into_owned()},
        "image_base64":if image{Some(STANDARD.encode(body))}else{None},
        "stderr":trace(&output.stderr)})
}

fn split_curl_output<'a>(output: &'a [u8], marker: &[u8]) -> (&'a [u8], Value) {
    match output
        .windows(marker.len())
        .rposition(|bytes| bytes == marker)
    {
        Some(index) => (
            &output[..index],
            serde_json::from_slice(&output[index + marker.len()..]).unwrap_or(Value::Null),
        ),
        None => (output, Value::Null),
    }
}
