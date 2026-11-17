# 配置与服务

[返回项目首页](../README.md)

右上角可新增、切换 VPN 配置。每套配置独立保存 OpenVPN 参数、endpoint 列表与排序、DNS、代理/SSH 认证和健康检查；全部放在同一个 YAML 中。管理认证通过容器环境变量配置。**同一时间运行一套 VPN 配置**，切换会关闭旧 SSH 会话、停止旧服务并应用新配置；新增本身不会切换当前连接。

- **OpenVPN**：粘贴标准 `.ovpn` 原文或手动选项；CA、客户端证书、私钥、`tls-auth`、`tls-crypt` 使用多行文本框。内联证书会在保存时提取到独立文件。
- **HTTP**：Tinyproxy，支持关闭、无认证、用户名密码认证；Tinyproxy 的密码不能包含空白字符。
- **SOCKS5**：Dante，支持关闭、无认证、用户名密码认证；支持 TCP CONNECT，不提供 UDP ASSOCIATE。等待 VPN 网卡出现后监听。
- **SSH**：OpenSSH，支持密码、公钥或同时启用。密码为空时关闭密码登录；支持配置多个公钥，每行一个，任意一个已配置公钥都可用于该用户登录。用户名和密码输入框关闭浏览器自动填充，点击后即可编辑。支持 SFTP、TCP 转发、`GatewayPorts clientspecified`、TUN 转发。免密 sudo 默认关闭，开启后此用户具有容器 root 权限。
- **管理和日志**：概览显示服务状态和固定端口的监听状态，提供启动、停止、重启及配置入口。日志页可查看和下载近期输出。

顶部“保存配置”只写磁盘；服务页“保存并重启”应用对应服务。容器重新启动后按保存设置恢复服务；OpenVPN 另外受“启动时自动连接”控制。SOCKS5 与 SSH 的用户名不能相同。

右上角提供“跟随系统 / 亮色 / 暗色”主题切换，默认跟随系统并随系统外观自动变化，手动选择会保存在当前浏览器。界面支持手机和平板，窄屏顶部导航可横向滑动。

页面使用 Hash 路由，可直接访问 `/#/vpn`、`/#/tools`、`/#/health` 等地址，支持刷新与浏览器前进、后退。网络工具先选择再展示，每个工具都有独立地址：`/#/tools/ping`、`/#/tools/tcping`、`/#/tools/curl`、`/#/tools/dns`、`/#/tools/mtr`、`/#/tools/traceroute`。

代理请求示例需先在页面启用相应服务，并设置用户名和密码认证：

```bash
curl -x http://127.0.0.1:17888 --proxy-user 'proxyuser:请替换为代理密码' https://example.com
curl --proxy socks5h://127.0.0.1:17080 --proxy-user 'sockuser:password' https://example.com
ssh -p 17222 work@127.0.0.1
```

## Endpoint 与自动选择

每个 endpoint 可填写名称、域名或 IPv4、端口，以及 UDP/TCP。支持拖拽排序、上下移动和手动下拉选择，页面默认回显上次成功使用的 endpoint。

默认 **Auto**：先通过固定 IP 直连 HTTPDNS 解析域名，再对每个 IP＋端口执行真实 OpenVPN 握手验证。TCP 额外检测端口连接；UDP 不以“发出数据包”判断可用。可用项按 Ping 延迟选择最低项，TCP 在 Ping 不通时使用 TCPing；没有可测延迟的可用项按列表排序选择最上面一项。探测结果显示在页面中，全部失败会报告错误并保留出口保护。

手动模式只连接指定 endpoint。填写 endpoint 列表后，它接管原文的 `remote`；列表留空时使用 `.ovpn` 中的 `remote`。Auto 每轮最多并发 4 项，每次握手探测最多 10 秒；许多候选项可能需要更长时间。

DNS、HTTPDNS 和网络工具见[DNS 与网络](networking.md)；故障恢复见[健康检查](health-checks.md)；配置文件结构见[存储与权限](storage.md#yaml-结构)。
