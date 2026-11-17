use crate::config::ConfigDocument;
use std::fmt::Write;

struct Endpoint {
    method: &'static str,
    path: &'static str,
    summary: &'static str,
    details: &'static str,
}

// 两份文档共用接口描述；示例只使用模型默认值，不读取当前实例的配置或凭据。
const ENDPOINTS: &[Endpoint] = &[
    Endpoint {
        method: "GET",
        path: "/healthz",
        summary: "无需认证的容器存活检查",
        details: r#"无请求体。健康时 HTTP 200，返回 {"status":"ok"}；异常或检查超时为 HTTP 503，返回 {"status":"unhealthy"}。
只检查管理服务、健康检查任务和出口防火墙，不代表 VPN 已连接。不会返回配置或密钥。"#,
    },
    Endpoint {
        method: "GET",
        path: "/api/status",
        summary: "读取服务、VPN、DNS、端口和健康状态",
        details: r#"无请求体。返回对象包含：
- version、basic_auth_enabled：程序版本和管理认证是否启用。
- services：数组，每项含 name、state、pid、description；state 来自 Supervisor，如 RUNNING、STOPPED、STARTING。
- vpn：state、ip，连接时还可能包含 detail、since（Unix 秒）。CONNECTED 表示隧道已连接。
- kill_switch：出口防火墙默认拒绝策略是否就绪。
- ports：数组，每项含 name、port、listening、published_port、target_port；published_port 未配置时为 null。
- routes：容器 IPv4 主路由表，常见字段为 dst、gateway、dev。
- active_profile：当前已应用的配置 ID。
- dns：mode、pushed、effective、source，以及可能的 detected_at。
- httpdns：启动探测 state、results、checked_at；httpdns_presets 为可用预设。
- endpoint_probes：候选节点的 ip、port、protocol、endpoint_id、ok、latency_ms、method、error。
- selected_endpoint：当前选中节点对象或 null；remote_endpoints 为运行时候选节点。
- health.container：healthy、errors。
- health.vpn：state、ok、healthy、errors、probe、consecutive_failures、last_action。
VPN 健康 state 为 unconfigured、stopped、connecting、checking、healthy 或 unhealthy；healthy 可能为 null。"#,
    },
    Endpoint {
        method: "GET",
        path: "/api/config",
        summary: "读取全部已保存配置及完整证书材料",
        details: r#"无请求体。直接返回 ConfigDocument（active_profile、profiles），结构见下方配置模型。
证书与私钥字段返回实际文本，不是磁盘上的引用 ID；响应也包含密码。分享诊断结果前应脱敏。
selected_endpoint 会回显该配置最后保存的有效选择。此结果用于编辑，实际运行状态以 /api/status 为准。"#,
    },
    Endpoint {
        method: "PUT",
        path: "/api/config",
        summary: "整体保存配置，不重启服务",
        details: r#"请求体是完整 ConfigDocument。先 GET /api/config，修改需要的字段，再 PUT 完整对象。
这不是 PATCH；缺失字段会按模型默认值补齐，不会自动保留原值，未知配置字段会被拒绝。
校验全部 profile，提取内联证书并持久化；不会自动连接 VPN 或重启代理。
成功返回 {"message":"配置已保存，应用或重启对应服务后生效","config":ConfigDocument}。
配置校验错误为 HTTP 400。请求体上限为 2 MiB。"#,
    },
    Endpoint {
        method: "POST",
        path: "/api/profiles",
        summary: "新增配置，不切换当前连接",
        details: r#"请求体：{"name":"调试 VPN"}。名称去除首尾空白后不能为空。
创建带 UUID 的默认配置，成功返回 {"id":"新配置 UUID","config":ConfigDocument}。
新增不会切换或重启当前连接。"#,
    },
    Endpoint {
        method: "POST",
        path: "/api/profiles/{id}/activate",
        summary: "切换配置并重新应用全部服务",
        details: r#"{id} 是已有 profile 的 UUID；无请求体。
先保存 active_profile，再重启并应用 OpenVPN、HTTP、SOCKS5、SSH，旧 SSH 会话会被终止。
成功返回 {"config":ConfigDocument}；不存在的 ID 为 HTTP 404。
应用失败时已保存的选择不会自动回滚，应重新读取配置、状态和日志确认实际结果。"#,
    },
    Endpoint {
        method: "POST",
        path: "/api/apply",
        summary: "重启并应用当前配置的全部服务",
        details: r#"无请求体。读取已保存的活动配置，重新应用 OpenVPN、HTTP、SOCKS5、SSH。
成功返回 {"message":"配置已应用"}。会中断现有连接；禁用的可选服务保持关闭。
自动选点包含真实 OpenVPN 握手，耗时可能较长；运行时操作最多等待 180 秒。"#,
    },
    Endpoint {
        method: "POST",
        path: "/api/services/{service}/{action}",
        summary: "启动、停止或重启指定服务",
        details: r#"无请求体。service：openvpn、http、socks5、sshd、backend；action：start、stop、restart。
backend 仅支持 restart，返回 {"message":"管理服务即将重启，请稍后刷新页面"} 后异步退出，由 Supervisor 拉起。
其他服务启动/重启会应用对应的已保存配置，返回 {"message":"配置已应用"}；停止返回 {"message":"服务已停止"}。
start 之前必须保存有效 VPN 配置或启用对应可选服务。SSH 停止/重启会终止现有 SSH 会话。
不支持的服务或操作为 HTTP 400。"#,
    },
    Endpoint {
        method: "POST",
        path: "/api/dns",
        summary: "热应用已保存的 DNS 设置",
        details: r#"无请求体。将已保存活动配置的 vpn.dns 和 vpn.dns_mode 应用到当前运行配置。
返回 {"message":"DNS 已热更新，VPN 和代理无需重启"}，不会重启 VPN 或代理。"#,
    },
    Endpoint {
        method: "POST",
        path: "/api/health",
        summary: "立即执行一次 VPN 健康探测",
        details: r#"无请求体。按已保存的健康检查设置探测，返回 state、ok、healthy、errors、probe 等字段。
state 与 /api/status 的 health.vpn 相同；未配置或手动停止时 healthy 可能为 null。
HTTP 200 仅表示请求执行成功，须检查 ok、healthy 和 errors 判断探测结果。
手动探测不更新周期结果、不累加失败次数，也不执行恢复动作。"#,
    },
    Endpoint {
        method: "GET",
        path: "/api/logs/{service}",
        summary: "读取最近的服务日志",
        details: r#"service：openvpn、http、socks5、sshd、backend。
可选查询参数 lines：非负整数，默认 200，实际限制为 1–1000。
示例：/api/logs/openvpn?lines=300。
返回 {"service":"openvpn","content":"日志文本"}，文件尚未创建时 content 为空字符串。
OpenVPN 日志会过滤内部管理 socket 的例行轮询，不过滤其他管理事件及 VPN 错误。"#,
    },
    Endpoint {
        method: "POST",
        path: "/api/tools/dns",
        summary: "比较默认 DNS、已应用 VPN DNS 与 HTTPDNS",
        details: r#"请求体：{"domain":"example.com"}。
返回 {"domain":"example.com","default_dns":[],"vpn_dns":[],"httpdns":[]}。
每项含 provider、ip、record_type（A/AAAA）、ok、answers、latency_ms，可能含 provider_id、url、protocol、error。
vpn_dns 只测试当前已应用的 DNS 地址；未应用时可能为空数组。
每条结果分别判断 ok；本操作不修改 DNS。"#,
    },
    Endpoint {
        method: "POST",
        path: "/api/tools/tcping",
        summary: "测试 TCP 连接",
        details: r#"请求体：{"host":"example.com","port":443}。port 必须为 1–65535。
使用容器当前默认 DNS 解析 IPv4，然后连接目标，连接超时为 5 秒。
返回 target、ip、ok、elapsed_ms、stdout、stderr、exit_code；解析失败时不含 ip。
探测失败也可能返回 HTTP 200，须检查 ok 和 stderr。"#,
    },
    Endpoint {
        method: "POST",
        path: "/api/tools/curl",
        summary: "测试 HTTP/HTTPS 请求与响应",
        details: r#"请求体：{"url":"https://example.com/","method":"GET"}。
method 必填且只允许 GET 或 HEAD；URL 只允许 HTTP/HTTPS，不允许认证信息或 # 片段。
使用容器当前默认 DNS 解析 IPv4，直接访问目标，遵循容器出口规则；不是通过 HTTP/SOCKS5 代理访问。
连接超时 5 秒，请求上限 20 秒，响应体上限 2 MiB，不自动跟随重定向。
返回 target、ip、method、ok、elapsed_ms、exit_code、status_code、content_type、response_headers、stdout、stderr、image_base64。
GET 图片时正文在 image_base64，stdout 为空；普通文本在 stdout；stderr 是详细连接日志。失败早期部分字段可能不存在。
ok 表示 curl 进程成功，不保证目标 HTTP 状态为 2xx，须同时检查 status_code。"#,
    },
    Endpoint {
        method: "GET (WebSocket)",
        path: "/api/tools/ping",
        summary: "实时 Ping，最长 60 秒",
        details: r#"查询参数 host 必填，例如 /api/tools/ping?host=example.com。
必须使用 WebSocket Upgrade；不能作为普通 JSON GET 调用。协议见下方 WebSocket 说明。"#,
    },
    Endpoint {
        method: "GET (WebSocket)",
        path: "/api/tools/trace",
        summary: "实时 Ping、MTR 或 Traceroute",
        details: r#"查询参数 tool、host 必填；tool 为 ping、mtr、traceroute。
示例：/api/tools/trace?tool=mtr&host=example.com。
必须使用 WebSocket Upgrade；协议见下方 WebSocket 说明。"#,
    },
];

fn introduction() -> String {
    format!(
        r#"# OpenVPN Web API

> OpenVPN、HTTP/SOCKS5 代理和 SSH 管理接口，供 AI 查阅与调试。程序版本：{}。

本文由后端生成，仅包含接口说明和示例，不读取实例配置、密码、证书或私钥。
所有路径相对于当前站点根地址；API 前缀为 /api，Hash 页面路由 /#/... 不是 API。
默认本机后端地址为 http://127.0.0.1:18080，Vite 开发入口为 http://127.0.0.1:5173，部署时以实际地址为准。

## 文档入口

- [接口索引](/llms.txt)：简版。
- [完整接口说明](/llms-full.txt)：参数、响应、配置模型和调试示例。
- /llm.txt 通过 HTTP 302 跳转到 /llms.txt；/llm-full.txt 通过 HTTP 302 跳转到 /llms-full.txt。
- 两份文档均支持 GET/HEAD，Content-Type 为 text/plain; charset=utf-8，沿用管理认证。

## 调用约定

- 管理认证可选；ADMIN_USERNAME 和 ADMIN_PASSWORD 同时设置时，除 /healthz 外均需 HTTP Basic Auth，包括本文和 WebSocket 握手。管理凭据与代理/SSH 凭据独立。
- JSON 请求发送 Content-Type: application/json；没有请求体的操作无需发送 {{}}。
- 写请求和 WebSocket 校验来源：提供 Origin 时必须与请求 Host 的主机和端口一致；Sec-Fetch-Site 为 cross-site 或 same-site 时会被拒绝。命令行客户端可不发送这些浏览器来源头。
- 写请求最多 2 MiB。保存配置与应用服务是两步；读取状态和日志后再选择需要的操作。

"#,
        env!("CARGO_PKG_VERSION")
    )
}

pub async fn index() -> String {
    let mut text = introduction();
    text.push_str("## 接口索引\n\n| 方法 | 路径 | 用途 |\n| --- | --- | --- |\n");
    for endpoint in ENDPOINTS {
        writeln!(
            text,
            "| {} | {} | {} |",
            endpoint.method, endpoint.path, endpoint.summary
        )
        .unwrap();
    }
    text.push_str(
        "\n## 调试顺序\n\n1. GET /healthz 区分容器故障；GET /api/status 查看 VPN、服务和防火墙。\n2. GET /api/logs/openvpn?lines=300 查看连接错误，再按需查询其他服务日志。\n3. POST /api/tools/dns、/api/tools/tcping 或 /api/tools/curl 定位解析和出口问题；HTTP 200 不一定代表探测成功，检查结果中的 ok。\n4. 需要修改时先 GET /api/config，修改后 PUT 完整对象，再热应用 DNS 或重启对应服务。配置含密码和私钥，分享前脱敏。\n5. 请求参数、响应字段、WebSocket 协议及配置示例请读取 /llms-full.txt。\n",
    );
    text
}

pub async fn full() -> String {
    let mut text = introduction();
    text.push_str("## 接口详情\n\n");
    for endpoint in ENDPOINTS {
        writeln!(
            text,
            "### {} {}\n\n{}\n\n{}\n",
            endpoint.method, endpoint.path, endpoint.summary, endpoint.details
        )
        .unwrap();
    }
    text.push_str(
        r#"## 配置模型

ConfigDocument 顶层仅包含 active_profile 和 profiles。每个 profile 包含 id（唯一 UUID）、name（非空名称）、settings；active_profile 必须引用现有 profile。
settings 包含 vpn、http、socks5、sshd、health。以下默认示例由 Rust 配置模型直接序列化，示例 UUID 固定以便复用；修改真实实例时保留 GET 返回的 ID 和其他字段。

```json
"#,
    );
    let mut example = ConfigDocument::default();
    let id = "00000000-0000-4000-8000-000000000001";
    example.active_profile = id.into();
    example.profiles[0].id = id.into();
    text.push_str(&serde_json::to_string_pretty(&example).unwrap());
    text.push_str(
        r#"
```

### 字段与约束

- vpn.config：OpenVPN 原文。ca、cert、key、tls_auth、tls_crypt 为文本材料；保存会提取内联证书。tls_auth 和 tls_crypt 不能同时设置，tls_auth_direction 为 0 或 1。
- vpn.endpoints：节点数组，每项必须包含 id、name、host、port、protocol；id 非空且在当前列表唯一，host 为域名或 IPv4，port 为 1–65535，protocol 为 udp 或 tcp。
- vpn.selected_endpoint：节点列表非空时必须引用有效节点，即使 selection_mode 为 auto 也要填写。selection_mode 为 auto 或 manual；列表接管原文 remote，空列表使用原文 remote。
- vpn.route_mode：all（默认全隧道）或 config（按 OpenVPN 配置路由）。vpn.autoconnect 只控制容器启动时自动连接。
- vpn.username、vpn.password：同时设置或同时留空。
- vpn.dns_mode：manual 或 pushed；manual 时 vpn.dns 至少含一个 IPv4。httpdns_url 为 HTTP/HTTPS 地址，httpdns_ips 为非空 IPv4 数组，httpdns_protocol 为 doh 或 json。
- http、socks5：enabled 控制启用；authentication 为 none 或 password。none 时 username/password 必须为空，password 时两者必填；HTTP 密码不能包含空白字符。
- 代理和 SSH 用户名以小写字母开头，仅含小写字母、数字、下划线、短横线，最长 32 位，禁止系统保留账户；SOCKS5 与 SSH 用户名不能相同。
- sshd：enabled、username、password、authorized_keys、allow_login、sudo。启用时至少有密码或一个公钥；authorized_keys 支持多行。allow_login 默认 false，仅允许转发；启用后允许终端、命令和 SFTP。sudo 控制免密 sudo。
- health.kind：ping、tcp、http；target 为 Ping/TCP 主机，port 为 TCP 端口，url 为 HTTP 检查地址；http_method 为 GET 或 HEAD，expected_status 为 100–599。
- health.interval_secs、failure_threshold、ping_count 必须大于 0，timeout_secs 为 1–60，port 为 1–65535。action 为 none、reconnect、restart、reapply；action_cooldown_secs 为 1–86400。
- 管理认证由环境变量配置，不属于 ConfigDocument。容器端口固定为 Web 80、HTTP 8888、SOCKS5 1080、SSH 22，不能在 profile 中添加 port。

## WebSocket 网络工具

使用 ws:// 或 wss://，路径和查询参数见接口详情。host 是域名或 IPv4，不含协议、端口或路径；国际化域名使用 Punycode。查询参数应进行 URL 编码。
连接建立后立即开始探测，无需发送启动消息；每约 2 秒接收一条 JSON 文本帧，stdout/stderr 是累计输出，应替换旧值而非追加。

```json
{"tool":"mtr","target":"example.com","ip":"93.184.215.14","elapsed_ms":2000,"stdout":"累计输出","stderr":"","done":false,"ok":false,"timed_out":false}
```

ip 在解析前或解析失败时为 null。中间帧 ok 不代表最终结果；done=true 表示结束，随后服务器发送关闭码 1000。
最长运行 60 秒；到达上限时 timed_out=true、ok=false，即使此前已有 Ping 回包。主动关闭连接会终止探测进程。
这些工具使用容器默认 DNS 和 IPv4。浏览器 WebSocket 自动携带已有 Basic 认证；命令行客户端可在握手中发送 Authorization，并提供与 Host 匹配的 Origin。

浏览器同源调用示例（在已登录的控制台页面执行）：

```javascript
const url = new URL('/api/tools/trace', location.href);
url.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
url.search = new URLSearchParams({ tool: 'mtr', host: 'example.com' }).toString();
const socket = new WebSocket(url);
socket.onmessage = ({ data }) => console.log(JSON.parse(data));
// 提前停止：socket.close();
```

## HTTP 错误处理

业务错误通常返回 {"error":"错误原因"}。JSON/查询参数提取、请求体限制或 WebSocket 升级失败也可能返回纯文本，应先判断 HTTP 状态和 Content-Type。

| HTTP 状态 | 含义与处理 |
| --- | --- |
| 200 | 请求完成；探测结果仍需检查 ok、healthy、status_code 或每条 DNS 结果 |
| 302 | 单数文档地址跳转到复数规范地址，Location 为站内相对路径 |
| 400 | 配置、参数、服务操作或 WebSocket 握手不合法 |
| 401 | 管理认证缺失或不匹配；响应含 WWW-Authenticate |
| 403 | Origin/Host 不匹配或 Sec-Fetch-Site 不允许 |
| 404 | API 不存在或激活的 profile 不存在 |
| 405 | HTTP 方法不支持 |
| 413 | 请求体超过 2 MiB |
| 415 / 422 | JSON Content-Type 不正确或请求字段/类型不符合模型 |
| 409 | 运行时服务操作失败；读取 error，核对保存配置、实际状态和服务日志 |
| 500 | 文件、子进程或运行时响应错误；查看 backend 日志 |
| 503 | /healthz 的容器检查未通过 |
| 504 | 运行时操作超过 180 秒；先查询状态和日志，再判断是否重试 |

## 命令行调试示例

BASE_URL 替换为实际站点地址，开发时可使用 Vite 地址。同一个脚本的 API 和 Origin 使用同一地址。
以下使用 Bash；未启用管理认证时 auth 保持为空。启用时设置 ADMIN_USERNAME 和 ADMIN_PASSWORD 环境变量。

```bash
BASE_URL='http://127.0.0.1:18080'
auth=()
if [ -n "${ADMIN_USERNAME:-}" ]; then
  auth=(--user "$ADMIN_USERNAME:$ADMIN_PASSWORD")
fi
curl --fail-with-body --silent --show-error "$BASE_URL/healthz"
curl --fail-with-body --silent --show-error "${auth[@]}" "$BASE_URL/llms-full.txt"
curl --fail-with-body --silent --show-error "${auth[@]}" "$BASE_URL/api/status"
curl --fail-with-body --silent --show-error "${auth[@]}" "$BASE_URL/api/logs/openvpn?lines=300"
curl --fail-with-body --silent --show-error "${auth[@]}" \
  -H "Origin: $BASE_URL" -H 'Content-Type: application/json' \
  -d '{"domain":"example.com"}' "$BASE_URL/api/tools/dns"
curl --fail-with-body --silent --show-error "${auth[@]}" \
  -H "Origin: $BASE_URL" -H 'Content-Type: application/json' \
  -d '{"host":"example.com","port":443}' "$BASE_URL/api/tools/tcping"
curl --fail-with-body --silent --show-error "${auth[@]}" \
  -H "Origin: $BASE_URL" -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com/","method":"HEAD"}' "$BASE_URL/api/tools/curl"
```

修改配置：先 GET /api/config，将完整 JSON 保存在权限为 0600 的本地文件中，修改需要的字段，PUT /api/config 提交完整文件，然后按需调用 POST /api/dns 或 POST /api/services/{service}/restart。
只有确实需要重启全部服务时才调用 POST /api/apply；它会中断现有连接。

## 故障定位

1. /healthz 失败：检查 backend、health 进程及 nftables；/healthz 成功仍可能 VPN 未连接。
2. /api/status 中 vpn 未 CONNECTED：读取 OpenVPN 日志、endpoint_probes、selected_endpoint，核对证书、认证和节点协议。
3. 已连接但域名失败：对比 /api/tools/dns 的 default_dns、vpn_dns、httpdns，以及状态中的 dns.effective；保存 DNS 后用 /api/dns 热应用。
4. 域名解析成功但访问失败：TCPing 目标端口，CURL 检查 status_code、response_headers、stderr；必要时使用 Ping、MTR 或 Traceroute。
5. 代理或 SSH 无法连接：核对 services、ports、对应 enabled/认证设置、宿主端口映射和对应服务日志。网络工具在容器内直连，不能替代代理认证测试。
6. VPN 断开后的出站失败属于出口保护的预期行为；不要通过关闭防火墙来恢复普通出口。
7. 分享诊断材料时移除 Authorization、密码、证书私钥及敏感地址；文档接口本身不包含这些实例数据。
"#,
    );
    text
}
