# DNS 与网络

[返回项目首页](../README.md)

## HTTPDNS 与 DNS 热更新

首选默认腾讯 DNSPod，并提供阿里 AliDNS、Cloudflare、Google、Quad9。预设均有可直接连接的 IPv4，采用 HTTPS DNS wire format（DoH）；也可自定义 URL、固定 IP 和 Google 风格 DNS JSON 协议。

所有 HTTPDNS 请求绑定容器 bridge 网卡，经 Docker 网关直连已允许的固定 IP，并保留域名与 TLS 证书验证，不依赖系统 DNS；VPN 全隧道路由不会改变这条解析路径。启动后异步测试各服务，不阻塞管理界面，结果按成功状态和耗时排序。首选不可用时依次尝试已探测可用的服务；尚无结果时并行探测全部预设并选择最快可用项。全部失败时明确报错，不回退到系统 DNS。内网 DNS 与公网 HTTPDNS 可能返回不同的地址。

OpenVPN 连接及重连时由 Rust hook 收集
`dhcp-option DNS`。运行配置优先接收这些地址，避免原 `.ovpn` 的 `pull-filter ignore/reject` 将其过滤；手动模式仍收集并展示下发地址，但使用手动填写的 DNS。后台分别展示服务器下发地址和当前生效地址，自动模式隐藏手动输入框，按钮与提示随模式切换。点击“应用服务器 DNS”或“应用手动 DNS”直接改写容器内
`/etc/resolv.conf`，**不重启容器、VPN 或代理**；已存在的应用 DNS 缓存不会强制刷新。自动模式没有下发 DNS 时不自行回退到公网 DNS，应用按钮显示等待状态。概览页展示全部当前生效 DNS 及其来源。

## 网络工具

网络工具页可输入域名，一次并行测试全部 HTTPDNS 直连 IP、自定义 HTTPDNS、VPN 下发 DNS 和手动 DNS，逐项显示答案、耗时与错误。VPN DNS 需要相应地址可达。

网络工具页还提供 Ping（4 次 ICMP）、TCPing（5 秒 TCP 连接超时）和 CURL GET/HEAD 探测。CURL 使用
`-vvv`，展示响应内容、响应头、连接详情、HTTP 状态和耗时；GET 单独展示响应头，图片响应直接预览，HEAD 只展示响应头。CURL 单次请求最多 20 秒、正文最多 2 MiB，详细日志保留前后共 64 KiB。工具不保存配置，不需要“保存配置”操作；探测从容器发起并遵循当前出口保护规则，域名经所配置的 HTTPDNS 解析。

MTR 和 Traceroute 通过同源 WebSocket 连接探测，由服务端每 2 秒推送累计结果，完成后发送最终结果并关闭连接，单次最多 60 秒（含目标解析）。两者均关闭域名反解析；MTR 展示每跳 IP、发送与回包数、未回包比例和延迟，Traceroute 展示路由追踪输出。暂停仅冻结前端展示，后端继续运行；恢复时显示最新结果。终止、切换工具或离开页面会关闭连接，服务端同时终止探测及其子进程。反向代理需要支持
`/api/tools/trace` 的 WebSocket Upgrade。

## 出口保护与参数边界

网络应用启动前安装默认拒绝的 nftables 策略，安装失败即退出。代理与 SSH 的新出站连接只允许通过
`tun0`，VPN 断开后不会回退到默认网关；隧道外 IPv6 同样默认拒绝。管理入站连接和 SSH 回复使用连接标记，从 Docker 网关返回，避免 VPN 接管路由后失联。

隧道外例外限于管理连接回复，以及 root 发起的已解析 VPN IP/端口通信、endpoint Ping 和固定 HTTPDNS IP/TCP 端口请求。HTTPDNS 例外供启动异步探测、断线重连和工具页使用，持续保留；普通 UDP/TCP 53 系统 DNS 没有出口例外。

[开发镜像](development.md#supervisor-网络管理)还允许 Supervisor `9001` 的入站管理连接及回包，宿主机仅映射回环地址；同样不允许隧道外主动出站。

固定使用 `tun0`、IPv4 VPN 远端和全隧道路由；标准 `dev tun`
自动规范化。守护进程、日志、管理 socket、脚本及凭据路径由容器控制。配置中的冲突选项会提示移除，不执行外部脚本/插件，不支持二次
`config` 引用、`<connection>` 分组、`route-nopull`
或交互解密私钥。原文引用外部证书文件时，把完整内容粘贴到对应文本框。支持 `tls-auth`
方向设置。无认证代理与 Basic Auth 的部署范围应按实际网络设置；需要通过公网管理时可在外部反向代理配置 HTTPS。

Endpoint 的自动选择见[配置与服务](configuration.md#endpoint-与自动选择)；Docker 网络限制和端口映射见[部署与启动](deployment.md#端口映射与-docker-网络)。
