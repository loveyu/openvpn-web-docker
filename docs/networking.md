# DNS 与网络

[返回项目首页](../README.md)

## HTTPDNS 与 DNS 热更新

首选默认腾讯 DNSPod，并提供阿里 AliDNS、Cloudflare、Google、Quad9。预设均有可直接连接的 IPv4，采用 HTTPS DNS wire format（DoH）；也可自定义 URL、固定 IP 和 Google 风格 DNS JSON 协议。

所有 HTTPDNS 请求绑定容器 bridge 网卡，经 Docker 网关直连已允许的固定 IP，并保留域名与 TLS 证书验证，不依赖系统 DNS；VPN 全隧道路由不会改变这条解析路径。启动后异步测试各服务，不阻塞管理界面，结果按成功状态和耗时排序。首选不可用时依次尝试已探测可用的服务；尚无结果时并行探测全部预设并选择最快可用项。全部失败时明确报错，不回退到系统 DNS。内网 DNS 与公网 HTTPDNS 可能返回不同的地址。

OpenVPN 连接及重连时由 Rust hook 收集
`dhcp-option DNS`。运行配置优先接收这些地址，避免原 `.ovpn` 的 `pull-filter ignore/reject` 将其过滤；手动模式仍收集并展示下发地址，但使用手动填写的 DNS。后台分别展示服务器下发地址和当前生效地址，自动模式隐藏手动输入框，按钮与提示随模式切换。点击“应用服务器 DNS”或“应用手动 DNS”直接改写容器内
`/etc/resolv.conf`，**不重启容器、VPN 或代理**；已存在的应用 DNS 缓存不会强制刷新。自动模式没有下发 DNS 时不自行回退到公网 DNS，应用按钮显示等待状态。概览页展示全部当前生效 DNS 及其来源。

## 网络工具

网络工具页可输入域名，一次并行测试容器默认 DNS、当前生效的 VPN DNS、全部 HTTPDNS 直连 IP 和自定义 HTTPDNS。结果从上到下按默认 DNS、VPN DNS、HTTPDNS 展示，逐项显示服务器 IP、答案、耗时与错误；移动端使用紧凑卡片，无需横向滚动。默认 DNS 遵循容器当前的 `/etc/resolv.conf`，可能与当前生效的 VPN DNS 返回相同结果。VPN DNS 仅测试已应用的地址：自动模式使用 VPN 下发的 DNS，手动模式使用已应用的手动 DNS；未启用或仅保存未应用的配置不参与，也不会在没有生效地址时回退到其他 DNS。VPN DNS 需要相应地址可达。

每台服务器分别并行查询 A（IPv4）和 AAAA（IPv6）记录，在同一条记录或移动端卡片内合并展示地址与各自耗时。未返回 IPv6 地址或 AAAA 查询失败时省略 IPv6 内容；任一类型返回地址即显示成功，两种类型均无地址时仅显示 IPv4 查询错误。AAAA 查询沿用现有 DNS 连接，不要求容器具有 IPv6 出口。VPN 启动选点及其他网络工具仍使用 IPv4 A 记录解析。

网络工具页还提供 Ping（持续 ICMP，最多 60 秒）、TCPing（5 秒 TCP 连接超时）和 CURL GET/HEAD 探测。CURL 使用
`-vvv`，展示响应内容、响应头、连接详情、HTTP 状态和耗时；GET 单独展示响应头，图片响应直接预览，HEAD 只展示响应头。CURL 单次请求最多 20 秒、正文最多 2 MiB，详细日志保留前后共 64 KiB。各工具的表单参数（目标地址、TCP 端口、HTTP 请求方法和 DNS 域名）修改后自动保存在当前浏览器的本地存储中，切换工具、刷新或再次打开同一站点时分别恢复，无需点击“保存配置”。清除站点数据后恢复初始默认值；本地存储不可用时仍可正常编辑和执行探测。Ping、TCPing、CURL、MTR 和 Traceroute 均通过容器当前默认 DNS 查询 IPv4 A 记录，与 DNS 对比页的默认 DNS 使用同一解析路径；解析失败直接报错，不回退到 HTTPDNS。探测从容器发起并遵循当前出口保护规则。DNS 对比工具仍查询各类 DNS，VPN endpoint 的启动与重连解析仍使用 HTTPDNS。

Ping、MTR 和 Traceroute 通过同源 WebSocket 连接探测，由服务端每 2 秒推送累计结果，完成后发送最终结果并关闭连接，单次最多 60 秒（含目标解析）。三者均关闭域名反解析；Ping 每秒发送一次 ICMP 请求，输出自动滚动到底部；MTR 展示每跳 IP、发送与回包数、未回包比例和延迟，Traceroute 展示路由追踪输出。暂停仅冻结前端展示，后端继续运行；恢复时显示最新结果。终止、切换工具或离开页面会关闭连接，服务端同时终止探测及其子进程。反向代理需要支持
`/api/tools/ping?host=目标` 和 `/api/tools/trace?tool=mtr或traceroute&host=目标` 的 WebSocket Upgrade。

## 出口保护与参数边界

网络应用启动前安装默认拒绝的 nftables 策略，安装失败即退出。代理与 SSH 的新出站连接只允许通过
`tun0`，VPN 断开后不会回退到默认网关；隧道外 IPv6 同样默认拒绝。管理入站连接和 SSH 回复使用连接标记，从 Docker 网关返回，避免 VPN 接管路由后失联。

隧道外例外限于管理连接回复，以及 root 发起的已解析 VPN IP/端口通信、endpoint Ping 和固定 HTTPDNS IP/TCP 端口请求。HTTPDNS 例外供启动异步探测、断线重连和工具页使用，持续保留；普通 UDP/TCP 53 系统 DNS 没有出口例外。

[开发镜像](development.md#supervisor-网络管理)还允许 Supervisor `9001` 的入站管理连接及回包，宿主机仅映射回环地址；同样不允许隧道外主动出站。

固定使用 `tun0` 和 IPv4 VPN 远端；标准 `dev tun` 自动规范化。路由模式默认“全部”（`vpn.route_mode: all`）：忽略原文的静态路由、`route-nopull`、路由优先级、重定向及限制路由的 `pull-filter ignore/reject`，统一生成 `redirect-gateway def1`。下发的静态路由和重定向同样忽略，但保留 subnet 拓扑需要的下发网关。“按配置”（`config`）保留原文与服务器下发的路由设置，不额外添加默认路由。

两种模式均保留现有出口保护；按配置未路由到 VPN 的目的地址不会自动放行到 Docker 网关。DNS 仍由页面单独管理。路由模式保存后重新连接才生效。

守护进程、日志、管理 socket、脚本及凭据路径由容器控制。配置中的冲突选项会提示移除，不执行外部脚本/插件，不支持二次
`config` 引用、`<connection>` 分组或交互解密私钥。原文引用外部证书文件时，把完整内容粘贴到对应文本框。支持 `tls-auth`
方向设置。无认证代理与 Basic Auth 的部署范围应按实际网络设置；需要通过公网管理时可在外部反向代理配置 HTTPS。

Endpoint 的自动选择见[配置与服务](configuration.md#endpoint-与自动选择)；Docker 网络限制和端口映射见[部署与启动](deployment.md#端口映射与-docker-网络)。
