mod api_docs;
mod config;
mod network;
mod runtime;
mod storage;
mod tools;

use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Path, Query, Request, State, WebSocketUpgrade, rejection::JsonRejection,
    },
    http::{HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use config::{Config, ConfigDocument, Profile, WEB_PORT};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    env, os::unix::fs::PermissionsExt, path::PathBuf, process::Stdio, sync::Arc, time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::{io::AsyncWriteExt, process::Command, sync::Mutex};
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

struct App {
    authorization: Option<String>,
    data: PathBuf,
    lock: Mutex<()>,
}

struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}
impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        tracing::error!(%error, "文件或子进程操作失败");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "文件或子进程操作失败，请检查服务日志".into(),
        )
    }
}
type ApiResult<T> = Result<T, ApiError>;

async fn load_config(app: &App) -> ApiResult<ConfigDocument> {
    Ok(storage::load(&app.data)?)
}

async fn runtime(app: &App, args: &[&str], input: Option<&Config>) -> ApiResult<Value> {
    let mut child = Command::new(env::current_exe()?)
        .arg("runtime")
        .args(args)
        .env("CONFIG_DIR", &app.data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    if let Some(config) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(config).unwrap())
            .await?;
    } else {
        drop(child.stdin.take());
    }
    let output = tokio::time::timeout(Duration::from_secs(180), child.wait_with_output())
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::GATEWAY_TIMEOUT,
                "服务操作超时，请检查当前状态和日志后重试".into(),
            )
        })??;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(ApiError(
            if args.first() == Some(&"validate") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::CONFLICT
            },
            message,
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "运行时返回了无效结果".into(),
        )
    })
}

async fn authorize(State(app): State<Arc<App>>, request: Request, next: Next) -> Response {
    let supplied = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if request.uri().path() != "/healthz"
        && app
            .authorization
            .as_ref()
            .is_some_and(|expected| !bool::from(expected.as_bytes().ct_eq(supplied.as_bytes())))
    {
        let mut response =
            ApiError(StatusCode::UNAUTHORIZED, "需要管理用户名和密码".into()).into_response();
        response.headers_mut().insert(
            "www-authenticate",
            HeaderValue::from_static("Basic realm=\"OpenVPN Web\", charset=\"UTF-8\""),
        );
        return response;
    }
    // Basic Auth 由浏览器自动携带；写请求和 WebSocket 均须校验来源。
    let websocket = request
        .headers()
        .get("upgrade")
        .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"));
    if websocket || !matches!(request.method().as_str(), "GET" | "HEAD" | "OPTIONS") {
        let cross_site = request
            .headers()
            .get("sec-fetch-site")
            .is_some_and(|value| value == "cross-site" || value == "same-site");
        let foreign_origin = request.headers().get("origin").is_some_and(|value| {
            let origin = value
                .to_str()
                .ok()
                .and_then(|s| s.parse::<axum::http::Uri>().ok());
            let host = request.headers().get("host").and_then(|h| h.to_str().ok());
            origin
                .as_ref()
                .and_then(|uri| uri.authority())
                .map(|a| a.as_str())
                != host
                || host.is_none()
        });
        if cross_site || foreign_origin {
            return ApiError(StatusCode::FORBIDDEN, "管理操作必须从同源页面发起".into())
                .into_response();
        }
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    for (key, value) in [
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        (
            "content-security-policy",
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'",
        ),
    ] {
        response
            .headers_mut()
            .insert(key, HeaderValue::from_static(value));
    }
    response
}

async fn get_config(State(app): State<Arc<App>>) -> ApiResult<Json<ConfigDocument>> {
    Ok(Json(storage::editor_load(&app.data)?))
}
async fn save_config(
    State(app): State<Arc<App>>,
    payload: Result<Json<ConfigDocument>, JsonRejection>,
) -> ApiResult<Json<Value>> {
    let Json(config) = payload.map_err(|error| {
        ApiError(
            StatusCode::BAD_REQUEST,
            format!("配置格式错误：{}", error.body_text()),
        )
    })?;
    let _guard = app.lock.lock().await;
    config
        .validate()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    for profile in &config.profiles {
        runtime(&app, &["validate"], Some(&profile.settings)).await?;
    }
    storage::save(&app.data, &config)?;
    Ok(Json(
        json!({"message": "配置已保存，应用或重启对应服务后生效", "config": load_config(&app).await?}),
    ))
}
async fn status(State(app): State<Arc<App>>) -> ApiResult<Json<Value>> {
    let mut value = runtime(&app, &["status"], None).await?;
    value["version"] = json!(env!("CARGO_PKG_VERSION"));
    value["basic_auth_enabled"] = json!(app.authorization.is_some());
    Ok(Json(value))
}
async fn apply(State(app): State<Arc<App>>) -> ApiResult<Json<Value>> {
    let _guard = app.lock.lock().await;
    Ok(Json(
        runtime(&app, &["apply", "all", "restart"], None).await?,
    ))
}

#[derive(Deserialize)]
struct NewProfile {
    name: String,
}
async fn create_profile(
    State(app): State<Arc<App>>,
    Json(input): Json<NewProfile>,
) -> ApiResult<Json<Value>> {
    let _guard = app.lock.lock().await;
    if input.name.trim().is_empty() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "VPN 配置名称不能为空".into(),
        ));
    }
    let mut document = load_config(&app).await?;
    let profile = Profile {
        name: input.name.trim().into(),
        ..Default::default()
    };
    let id = profile.id.clone();
    document.profiles.push(profile);
    storage::save(&app.data, &document)?;
    Ok(Json(json!({"config": load_config(&app).await?, "id": id})))
}
async fn activate_profile(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let _guard = app.lock.lock().await;
    let mut document = load_config(&app).await?;
    if !document.profiles.iter().any(|profile| profile.id == id) {
        return Err(ApiError(StatusCode::NOT_FOUND, "VPN 配置不存在".into()));
    }
    document.active_profile = id;
    storage::save(&app.data, &document)?;
    runtime(&app, &["apply", "all", "restart"], None).await?;
    Ok(Json(json!({"config": load_config(&app).await?})))
}
async fn apply_dns(State(app): State<Arc<App>>) -> ApiResult<Json<Value>> {
    let _guard = app.lock.lock().await;
    Ok(Json(runtime(&app, &["dns"], None).await?))
}

#[derive(Deserialize)]
struct DnsTest {
    domain: String,
}
async fn test_dns(
    State(app): State<Arc<App>>,
    Json(input): Json<DnsTest>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        runtime(&app, &["dns-test", &input.domain], None).await?,
    ))
}

async fn check_health(State(app): State<Arc<App>>) -> ApiResult<Json<Value>> {
    Ok(Json(runtime(&app, &["health"], None).await?))
}

#[derive(Deserialize)]
struct PingInput {
    host: String,
}
#[derive(Deserialize)]
struct TcpPingInput {
    host: String,
    port: u16,
}
#[derive(Deserialize)]
struct CurlInput {
    url: String,
    method: String,
}

fn invalid_tool_input(error: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, error.to_string())
}

async fn test_ping(
    Query(input): Query<PingInput>,
    websocket: WebSocketUpgrade,
) -> ApiResult<Response> {
    let host = network::domain(&input.host).map_err(invalid_tool_input)?;
    Ok(websocket
        .on_upgrade(move |socket| {
            tools::path_trace::run(socket, tools::path_trace::Tool::Ping, host)
        })
        .into_response())
}

async fn test_tcping(Json(input): Json<TcpPingInput>) -> ApiResult<Json<Value>> {
    let host = network::domain(&input.host).map_err(invalid_tool_input)?;
    if input.port == 0 {
        return Err(invalid_tool_input("TCP 端口必须为 1–65535"));
    }
    Ok(Json(tools::tcping(&host, input.port).await))
}

async fn test_curl(Json(input): Json<CurlInput>) -> ApiResult<Json<Value>> {
    if !matches!(input.method.as_str(), "GET" | "HEAD") {
        return Err(invalid_tool_input("CURL 仅支持 GET 和 HEAD"));
    }
    let (host, _) = network::http_target(&input.url).map_err(invalid_tool_input)?;
    network::domain(&host).map_err(invalid_tool_input)?;
    Ok(Json(tools::curl(&input.url, &input.method).await))
}

#[derive(Deserialize)]
struct TraceInput {
    tool: tools::path_trace::Tool,
    host: String,
}

async fn trace_path(
    Query(input): Query<TraceInput>,
    websocket: WebSocketUpgrade,
) -> ApiResult<Response> {
    let host = network::domain(&input.host).map_err(invalid_tool_input)?;
    Ok(websocket
        .on_upgrade(move |socket| tools::path_trace::run(socket, input.tool, host))
        .into_response())
}

async fn healthz(State(app): State<Arc<App>>) -> (StatusCode, Json<Value>) {
    let result = tokio::time::timeout(
        Duration::from_millis(2500),
        runtime(&app, &["container-health"], None),
    )
    .await;
    match result {
        Ok(Ok(value)) if value["healthy"] == true => (StatusCode::OK, Json(json!({"status":"ok"}))),
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status":"unhealthy"})),
        ),
    }
}
async fn control(
    State(app): State<Arc<App>>,
    Path((service, action)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    if !["start", "stop", "restart"].contains(&action.as_str())
        || !["openvpn", "http", "socks5", "sshd", "backend"].contains(&service.as_str())
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "不支持的服务或操作".into(),
        ));
    }
    let _guard = app.lock.lock().await;
    if service == "backend" {
        if action != "restart" {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "管理服务只支持重启".into(),
            ));
        }
        tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(500)).await;
            // Supervisor 负责重新拉起管理服务。
            std::process::exit(0);
        });
        return Ok(Json(json!({"message": "管理服务即将重启，请稍后刷新页面"})));
    }
    Ok(Json(
        runtime(&app, &["apply", &service, &action], None).await?,
    ))
}

#[derive(Deserialize)]
struct LogQuery {
    lines: Option<usize>,
}

fn visible_log_lines<'a>(content: &'a str, service: &str, limit: usize) -> Vec<&'a str> {
    content
        .lines()
        .rev()
        .filter(|line| {
            // 仅隐藏管理 socket 的例行状态轮询，保留其他管理事件及 VPN 诊断。
            service != "openvpn"
                || !line
                    .split_once(" MANAGEMENT: ")
                    .is_some_and(|(_, message)| {
                        matches!(
                            message,
                            "Client connected from /run/vpnweb/openvpn-management.sock"
                                | "CMD 'state'"
                                | "Client disconnected"
                        )
                    })
        })
        .take(limit)
        .collect()
}

async fn read_service_log(
    path: &std::path::Path,
    service: &str,
    limit: usize,
) -> std::io::Result<String> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(error),
    };
    let size = file.metadata().await?.len();
    let mut window = 65536;
    let mut bytes = Vec::new();
    loop {
        let start = size.saturating_sub(window);
        file.seek(std::io::SeekFrom::Start(start)).await?;
        bytes.clear();
        file.read_to_end(&mut bytes).await?;
        let content = String::from_utf8_lossy(&bytes);
        // 从行边界开始过滤，避免把截断的轮询记录当成有效日志。
        let content = if start > 0 {
            content.split_once('\n').map_or("", |(_, rest)| rest)
        } else {
            &content
        };
        let lines = visible_log_lines(content, service, limit);
        if start == 0 || lines.len() == limit {
            return Ok(lines.into_iter().rev().collect::<Vec<_>>().join("\n"));
        }
        // 噪声不占行数；按倍数扩大尾部窗口，找回被轮询挤出的诊断信息。
        window *= 2;
    }
}

async fn logs(
    State(app): State<Arc<App>>,
    Path(service): Path<String>,
    Query(query): Query<LogQuery>,
) -> ApiResult<Json<Value>> {
    if !["openvpn", "http", "socks5", "sshd", "backend"].contains(&service.as_str()) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "未知服务".into()));
    }
    let path = app.data.join("logs").join(format!("{service}.log"));
    let content =
        read_service_log(&path, &service, query.lines.unwrap_or(200).clamp(1, 1000)).await?;
    Ok(Json(json!({"service": service, "content": content})))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openvpn_web=info,tower_http=info".into()),
        )
        .init();
    let data = PathBuf::from(env::var("CONFIG_DIR").unwrap_or_else(|_| "/config".into()));
    let arguments = env::args().collect::<Vec<_>>();
    if arguments.get(1).map(String::as_str) == Some("runtime") {
        match runtime::dispatch(data, &arguments[2..]).await {
            Ok(value) => println!("{}", serde_json::to_string(&value)?),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    tokio::fs::create_dir_all(&data).await?;
    // SSH 用户需要穿过数据目录访问自己的 home；配置文件本身始终为 0600。
    tokio::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o711)).await?;
    let path = data.join("config.yaml");
    if !tokio::fs::try_exists(&path).await? {
        storage::save(&data, &ConfigDocument::default())?;
    }
    let config = storage::load(&data)?;
    config.validate().map_err(std::io::Error::other)?;
    let username = env::var("ADMIN_USERNAME").unwrap_or_default();
    let password = env::var("ADMIN_PASSWORD").unwrap_or_default();
    if username.is_empty() != password.is_empty() {
        return Err("ADMIN_USERNAME 和 ADMIN_PASSWORD 必须同时设置或同时留空".into());
    }
    if username.contains(':') {
        return Err("ADMIN_USERNAME 不能包含冒号".into());
    }
    let authorization = (!username.is_empty()).then(|| {
        format!(
            "Basic {}",
            STANDARD.encode(format!("{username}:{password}"))
        )
    });
    if env::args().any(|arg| arg == "--init") {
        return Ok(());
    }
    let app = Arc::new(App {
        authorization,
        data,
        lock: Mutex::new(()),
    });
    let api = Router::new()
        .route("/config", get(get_config).put(save_config))
        .route("/status", get(status))
        .route("/apply", post(apply))
        .route("/dns", post(apply_dns))
        .route("/tools/dns", post(test_dns))
        .route("/tools/ping", get(test_ping))
        .route("/tools/tcping", post(test_tcping))
        .route("/tools/curl", post(test_curl))
        .route("/tools/trace", get(trace_path))
        .route("/health", post(check_health))
        .route("/profiles", post(create_profile))
        .route("/profiles/{id}/activate", post(activate_profile))
        .route("/services/{service}/{action}", post(control))
        .route("/logs/{service}", get(logs))
        .fallback(|| async { ApiError(StatusCode::NOT_FOUND, "接口不存在".into()) });
    let static_dir = env::var("STATIC_DIR").unwrap_or_else(|_| "/app/frontend".into());
    let router = Router::new()
        .nest("/api", api)
        .route("/healthz", get(healthz))
        .route("/llms.txt", get(api_docs::index))
        .route("/llms-full.txt", get(api_docs::full))
        .route(
            "/llm.txt",
            get(|| async { (StatusCode::FOUND, [("location", "/llms.txt")]) }),
        )
        .route(
            "/llm-full.txt",
            get(|| async { (StatusCode::FOUND, [("location", "/llms-full.txt")]) }),
        )
        .fallback_service(
            ServeDir::new(&static_dir)
                .not_found_service(ServeFile::new(format!("{static_dir}/index.html"))),
        )
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn_with_state(app.clone(), authorize))
        .layer(middleware::from_fn(security_headers))
        .with_state(app.clone());
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, WEB_PORT)).await?;
    tracing::info!(port = WEB_PORT, "管理服务已启动");
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let mut term =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn openvpn_logs_hide_only_routine_management_polling() {
        let messages = [
            "Initialization Sequence Completed",
            "MANAGEMENT: Client connected from /run/vpnweb/openvpn-management.sock",
            "MANAGEMENT: CMD 'state'",
            "MANAGEMENT: Client disconnected",
            "MANAGEMENT: CMD 'signal SIGUSR1'",
            "MANAGEMENT: Client connected from [AF_INET]127.0.0.1:1234",
            "MANAGEMENT: Socket bind failed: Permission denied",
            "TLS Error: TLS handshake failed",
            "SIGUSR1[soft,tls-error] received, process restarting",
        ];
        let content = messages
            .map(|message| format!("2026-11-17 18:16:58 {message}"))
            .join("\n");
        let lines = visible_log_lines(&content, "openvpn", 100);
        assert_eq!(
            lines.into_iter().rev().collect::<Vec<_>>(),
            [0, 4, 5, 6, 7, 8].map(|i| format!("2026-11-17 18:16:58 {}", messages[i]))
        );
        assert_eq!(visible_log_lines(&content, "backend", 100).len(), 9);
    }

    #[tokio::test]
    async fn openvpn_log_limit_counts_diagnostics_before_a_large_polling_tail() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("openvpn.log");
        let noise = "2026-11-17 18:16:58 MANAGEMENT: Client connected from /run/vpnweb/openvpn-management.sock\n\
                     2026-11-17 18:16:58 MANAGEMENT: CMD 'state'\n\
                     2026-11-17 18:16:58 MANAGEMENT: Client disconnected\n";
        let older = "2026-11-17 18:16:57 Initialization Sequence Completed";
        let newer = "2026-11-17 18:17:03 TLS Error: TLS handshake failed";
        fs::write(
            &path,
            format!("旧诊断\n{older}\n{}{newer}\n{noise}", noise.repeat(1024)),
        )
        .unwrap();
        assert_eq!(
            read_service_log(&path, "openvpn", 2).await.unwrap(),
            format!("{older}\n{newer}")
        );
    }
}
