# OpenVPN Web Docker

一个容器替换 Gluetun、SOCKS5 和 OpenSSH 组合，提供 Rust 后端与 React 管理界面。管理 OpenVPN 连接、HTTP/SOCKS5 代理、可选 SSH、多套 VPN 配置、DNS、端口和服务日志。

自有 API、配置生成、网络探测、防火墙、DNS hook 和健康检查全部使用 Rust。Python 仅作为 Supervisor 及其 supervisorctl 的依赖；镜像没有自有 Python 应用。OpenVPN、Tinyproxy、Dante 和 OpenSSH 直接集成到镜像中。

## 启动

需要 Linux Docker Engine、Compose 插件、`/dev/net/tun` 和内核 nftables 支持。使用独立 **bridge 网络，不支持 host 模式或共享其他容器的网络命名空间**；防火墙与路由只作用于本容器。

```bash
cp .env.example .env
# 可选：编辑 .env，设置 ADMIN_USERNAME 和 ADMIN_PASSWORD
# 默认均为空，不启用管理认证
docker compose up -d --build
```

访问 <http://127.0.0.1:18080>。在 OpenVPN 页面粘贴或导入 `client.ovpn`，填写证书、私钥及所需凭据，配置 endpoint 后点击“保存并连接”。默认手动 DNS 为 `1.1.1.1`，请按 VPN 网络要求修改，也可切换到“使用 OpenVPN 下发 DNS”。SOCKS5 和 SSH 默认关闭，在各自页面启用。

| 服务 | 默认状态 | 容器端口 | 默认宿主机端口 |
| --- | --- | --- | --- |
| 管理页面 / API | 开启，可选 Basic Auth | 8080 | 18080 |
| HTTP / HTTPS CONNECT | 开启，无认证 | 8888 | 17888 |
| SOCKS5 TCP CONNECT | 关闭 | 1080 | 17080 |
| SSH / SFTP / 转发 | 关闭 | 2222 | 17222 |

Compose 默认绑定宿主机 `127.0.0.1`。局域网访问时设置 `.env` 的 `BIND_ADDRESS`；例如宿主机局域网 IP。管理 Basic Auth 通过 `ADMIN_USERNAME`、`ADMIN_PASSWORD` 环境变量设置，启用时必须同时设置两项，修改后重新创建容器。浏览器使用原生认证对话框，API 使用 `curl -u '用户名:密码'`。`/healthz` 不要求认证，返回健康状态，不包含配置或密钥。

## 配置和服务操作

右上角可新增、切换 VPN 配置。每套配置独立保存 OpenVPN 参数、endpoint 列表与排序、DNS、代理/SSH 端口、认证和健康检查；全部放在同一个 YAML 中。管理页面端口与管理认证属于全局配置。**同一时间运行一套 VPN 配置**，切换会关闭旧 SSH 会话、停止旧服务并应用新配置；新增本身不会切换当前连接。

- **OpenVPN**：粘贴标准 `.ovpn` 原文或手动选项；CA、客户端证书、私钥、`tls-auth`、`tls-crypt` 使用多行文本框。内联证书会在保存时提取到独立文件。
- **HTTP**：Tinyproxy，支持关闭、无认证、用户名密码认证；Tinyproxy 的密码不能包含空白字符。
- **SOCKS5**：Dante，支持关闭、无认证、用户名密码认证；支持 TCP CONNECT，不提供 UDP ASSOCIATE。等待 VPN 网卡出现后监听。
- **SSH**：OpenSSH，支持密码、公钥或同时启用。密码为空时关闭密码登录；公钥每行一个。支持 SFTP、TCP 转发、`GatewayPorts clientspecified`、TUN 转发。免密 sudo 默认关闭，开启后此用户具有容器 root 权限。
- **管理和日志**：概览显示服务状态、当前监听端口和待应用端口，提供启动、停止、重启及配置入口。日志页可查看和下载近期输出。

顶部“保存配置”只写磁盘；服务页“保存并重启”应用对应服务；端口页“保存并应用服务”应用 VPN 和全部代理/SSH。管理端口需单独重启管理服务。容器重新启动后按保存设置恢复服务；OpenVPN 另外受“启动时自动连接”控制。SOCKS5 与 SSH 的用户名不能相同。

```bash
curl -x http://127.0.0.1:17888 https://example.com
curl --proxy socks5h://127.0.0.1:17080 --proxy-user 'sockuser:password' https://example.com
ssh -p 17222 work@127.0.0.1
```

### Endpoint 与自动选择

每个 endpoint 可填写名称、域名或 IPv4、端口，以及 UDP/TCP。支持拖拽排序、上下移动和手动下拉选择，页面默认回显上次成功使用的 endpoint。

默认 **Auto**：先通过固定 IP 直连 HTTPDNS 解析域名，再对每个 IP＋端口执行真实 OpenVPN 握手验证。TCP 额外检测端口连接；UDP 不以“发出数据包”判断可用。可用项按 Ping 延迟选择最低项，TCP 在 Ping 不通时使用 TCPing；没有可测延迟的可用项按列表排序选择最上面一项。探测结果显示在页面中，全部失败会报告错误并保留出口保护。

手动模式只连接指定 endpoint。填写 endpoint 列表后，它接管原文的 `remote`；列表留空时使用 `.ovpn` 中的 `remote`。Auto 每轮最多并发 4 项，每次握手探测最多 10 秒；许多候选项可能需要更长时间。

### HTTPDNS 与 DNS 热更新

首选默认腾讯 DNSPod，并提供阿里 AliDNS、Cloudflare、Google、Quad9。预设均有可直接连接的 IPv4，采用 HTTPS DNS wire format（DoH）；也可自定义 URL、固定 IP 和 Google 风格 DNS JSON 协议。

所有请求使用固定 IP 连接并保留域名与 TLS 证书验证，不依赖系统 DNS。启动后异步测试各服务，不阻塞管理界面，结果按成功状态和耗时排序。首选不可用时依次尝试已探测可用的服务；尚无结果时并行探测全部预设并选择最快可用项。全部失败时明确报错，不回退到系统 DNS。

OpenVPN 连接及重连时由 Rust hook 收集 `dhcp-option DNS`。后台展示下发地址和生效地址，可选择使用下发值或手动指定。点击“仅应用 DNS”直接改写容器内 `/etc/resolv.conf`，**不重启容器、VPN 或代理**；已存在的应用 DNS 缓存不会强制刷新。自动模式没有下发 DNS 时不自行回退到公网 DNS。

网络工具页可输入域名，一次并行测试全部 HTTPDNS 直连 IP、自定义 HTTPDNS、VPN 下发 DNS 和手动 DNS，逐项显示答案、耗时与错误。VPN DNS 需要相应地址可达。

### 健康检查

容器与 OpenVPN 的健康状态分别报告，概览和健康检查页同时展示，OpenVPN 页展示隧道健康状态：

- **容器健康**：`/healthz` 供 Docker 使用，仅检查管理进程、VPN 健康检查任务及出口防火墙的默认拒绝策略；正常返回 `200`，内部故障返回 `503`。VPN 未配置、连接中、断线或手动停止均不直接影响容器健康。HTTP/SOCKS5/SSH 的进程与监听状态另行展示。
- **OpenVPN 健康**：`/api/status` 的 `health.vpn` 区分未配置、已停止、连接中、检测中、正常、异常；检查实际连接状态与 `tun0` IPv4 地址，不受“启动时自动连接”开关影响。可选追加 Ping、TCP 或 HTTP 探测，固定通过 `tun0`，不会使用允许直连的 HTTPDNS 路径来证明隧道可用。

网络探测默认关闭。启用后，默认目标 `1.1.1.1`、TCP 端口 `443`、HTTP URL `https://www.qq.com`、GET、预期状态 `200`、间隔 `30` 秒、超时 `5` 秒、Ping 次数 `1`。超时限制整次探测，多个 Ping 共享该时间上限。结果随配置、连接变化失效，过期结果显示为等待检查；手动测试单独返回结果，不修改定时失败计数、不执行恢复动作。

**默认连续失败 `3` 次后重启内部 OpenVPN 服务**。连接故障和已启用的网络探测失败都会累计；成功后清零。界面可调整检查间隔、失败阈值、恢复动作和动作冷却时间（默认 `120` 秒）。动作可选：

| 动作 | 行为 |
| --- | --- |
| 仅记录 | 报告异常，不执行恢复 |
| 软重连 | 向 OpenVPN 发送 `SIGUSR1`，使用当前配置重新连接 |
| 重启 OpenVPN 服务（默认） | 通过 Supervisor 重启内部 OpenVPN 进程 |
| 重新应用 VPN 配置并重连 | 加载已保存的 VPN 配置并重连；Auto 模式重新探测选点，手动模式保留指定 endpoint |

每次动作后重新累计失败次数，并遵守冷却时间；最近动作、时间和结果显示在健康检查页。未配置 VPN、手动停止及配置切换期间不执行恢复。恢复动作只操作 OpenVPN，不重启容器或其他服务。Docker 的 `unhealthy` 标记本身不会触发 `restart: unless-stopped`。

### 端口修改与 Docker 网络

后台修改的是**容器监听端口**，`.env` 的 `WEB_PORT` 等修改的是**宿主机映射端口**。页面列出容器实际监听状态；Docker 的宿主机映射以 Compose 为准。

例如 SOCKS5 容器端口改为 `1081`：后台保存该端口，把 `compose.yaml` 映射右侧改为 `1081`，随后执行 `docker compose up -d --force-recreate`。新容器从保存配置加载端口。不同 VPN profile 使用不同端口时，应提前在 Compose 中发布全部需要的端口。容器不能自行变更 Docker 的端口映射。

**不要设置 `network_mode: host`**。当前实现依赖独立网络命名空间、IPv4 默认网关、连接标记和策略路由；也不提供其他容器通过 IP 转发共享隧道的网关模式。

## 存储与权限

只需挂载 `./config:/config`：

```text
/config/config.yaml                         # 全部 profile 与全局配置，0600
/config/secrets/<UUID>                       # 证书和私钥原文，root:root 0600
/config/logs/*.log                           # Supervisor 和各应用日志
/config/.state/                             # 上次 endpoint、账户 UID 等运行记录
/config/profiles/<profile-id>/ssh/home/      # 各配置独立 SSH home
/config/profiles/<profile-id>/ssh/ssh_host_* # 各配置独立 SSH 宿主密钥
```

`secrets`、`.state` 和日志目录为 root 所有、0700；YAML 中证书和私钥字段只保存 UUID，不存 PEM 原文。其他服务密码保存在 root 0600 的 YAML 中。SSH 用户使用独立 UID，无 sudo 时无法读取管理配置、私钥或其他 profile 的 home。管理员可在认证后的页面读取和编辑原文；宿主机 root 和授予 sudo 的 SSH 用户仍可访问这些内容。

运行配置与临时凭据在 `/run/vpnweb`，Compose 使用 tmpfs。日志持久化，Supervisor 自动轮转，每个主要服务最多约 15 MB。备份需要保留整个 `config` 目录及权限。旧密钥文件保留用于恢复，不会自动删除。

```bash
docker compose logs --tail=100
docker compose exec openvpn-web supervisorctl status
docker compose exec openvpn-web tail -n 100 /config/logs/bootstrap.log
docker compose exec openvpn-web tail -n 100 /config/logs/openvpn.log
```

### YAML 结构

首次启动自动生成完整默认值。建议通过界面编辑；需要手动改 YAML 时先停止容器，避免覆盖正在保存的内容。

```yaml
web:
  port: 8080
active_profile: <profile-uuid>
profiles:
  - id: <profile-uuid>
    name: 工作 VPN
    settings:
      vpn:
        config: |-
          client
          remote-cert-tls server
        selection_mode: auto
        selected_endpoint: endpoint-1
        endpoints:
          - id: endpoint-1
            name: 主节点
            host: vpn.example.com
            port: 1194
            protocol: udp
        ca: <secret-uuid>
        cert: <secret-uuid>
        key: <secret-uuid>
        dns_mode: manual
        dns: [1.1.1.1]
        httpdns_url: https://doh.pub/dns-query
        httpdns_ips: [1.12.12.12, 120.53.53.53]
        httpdns_protocol: doh
        autoconnect: true
      http:
        enabled: true
        port: 8888
      socks5:
        enabled: false
        port: 1080
      sshd:
        enabled: false
        port: 2222
      health:
        enabled: false
        failure_threshold: 3
        action: restart
        action_cooldown_secs: 120
```

以上是结构示例，UUID 占位值必须替换为有效的 profile / secret ID。密钥文件需要存在于 `secrets` 中。其余字段由程序补默认值。

## 出口保护与参数边界

网络应用启动前安装默认拒绝的 nftables 策略，安装失败即退出。代理与 SSH 的新出站连接只允许通过 `tun0`，VPN 断开后不会回退到默认网关；隧道外 IPv6 同样默认拒绝。管理入站连接和 SSH 回复使用连接标记，从 Docker 网关返回，避免 VPN 接管路由后失联。

隧道外例外限于管理连接回复，以及 root 发起的已解析 VPN IP/端口通信、endpoint Ping 和固定 HTTPDNS IP/TCP 端口请求。HTTPDNS 例外供启动异步探测、断线重连和工具页使用，持续保留；普通 UDP/TCP 53 系统 DNS 没有出口例外。

固定使用 `tun0`、IPv4 VPN 远端和全隧道路由；标准 `dev tun` 自动规范化。守护进程、日志、管理 socket、脚本及凭据路径由容器控制。配置中的冲突选项会提示移除，不执行外部脚本/插件，不支持二次 `config` 引用、`<connection>` 分组、`route-nopull` 或交互解密私钥。原文引用外部证书文件时，把完整内容粘贴到对应文本框。支持 `tls-auth` 方向设置。无认证代理与 Basic Auth 的部署范围应按实际网络设置；需要通过公网管理时可在外部反向代理配置 HTTPS。

## 版本和 GitHub CI

| 层级 | 版本 / 选型 |
| --- | --- |
| 后端 | Rust 1.100.0、Axum 0.8.9、Tokio 1.54.1 |
| 前端 | React 19.3.1、TypeScript 7.0.2、Vite 8.3.8 |
| 构建 Node | 26.13.1 LTS |
| 底包 | Debian 13 slim（trixie） |
| 守护 | Supervisor；Python 仅为其依赖 |
| 应用 | Debian 13 仓库的 OpenVPN、Tinyproxy、OpenSSH；官方源码 Dante 1.4.4 |

初始化采用当前最新兼容稳定版并保存 Cargo/npm 锁文件。系统程序使用 Debian 13 稳定仓库更新，不混用 unstable。Dante 因不在 Debian 13 仓库中，从官方源码校验 SHA256 后构建。

[GitHub 工作流](.github/workflows/docker.yml) 在 PR、main、`v*` 标签、手动和每周触发：

1. Rust fmt / Clippy / 单元测试，前端类型检查和生产构建。
2. AMD64 与 ARM64 原生 runner 分别构建完整镜像，执行隔离 Docker 集成测试。
3. 非 PR 运行在测试通过后发布 GHCR 多架构 manifest（latest、版本、提交 SHA）。

仓库需允许 Actions 写入 Packages；镜像公开性由 GHCR 仓库设置控制。首次成功发布后可在 `.env` 设置 `IMAGE=ghcr.io/<owner>/<repo>:latest`，执行 `docker compose pull` 和 `docker compose up -d --no-build`。本地不自动推送或发布。

## 开发与验证

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cd frontend
npm ci
npm run build
cd ..
docker build -t openvpn-web:local .
python3 scripts/smoke.py --image openvpn-web:local
docker compose config --quiet
```

Python smoke 脚本仅在开发机/CI 运行，不复制进镜像。它创建独立 Docker bridge、临时 TLS VPN 服务端、DNS/HTTP 测试夹具和客户端，验证 UDP 自动选点、HTTPDNS、DNS 热更新、代理与 SSH 认证、密钥权限、多配置端口隔离、健康检查、服务重启、断线保护和持久化；结束后清理自己的容器和网络。需要宿主机 `openssl`、`ssh`、`curl`、Python 3 和 Docker。
