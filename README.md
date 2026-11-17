# OpenVPN Web Docker

一个容器替换 Gluetun、SOCKS5 和 OpenSSH 组合，提供 Rust 后端与 React 管理界面。管理 OpenVPN 连接、HTTP/SOCKS5 代理、可选 SSH、多套 VPN 配置、DNS 和服务日志。

## 快速开始

支持 Linux（AMD64 / ARM64）及 macOS（Intel / Apple Silicon，使用 Docker Desktop 的 Linux 容器）。
需要 Docker Compose、Docker 引擎所在 Linux 环境的 `/dev/net/tun` 和 nftables 支持；macOS 的设备由 Docker Desktop 虚拟机提供。使用独立 bridge 网络，不支持 host 模式或共享其他容器的网络命名空间。

在仓库根目录创建环境变量文件，已有 `.env` 时保留原配置：

```bash
test -f .env || cp .env.example .env
```

示例默认配置管理用户名和强密码；无需认证时，将两项同时留空：

```dotenv
ADMIN_USERNAME=admin
ADMIN_PASSWORD='请替换为强密码'
```

构建并启动：

```bash
docker compose up -d --build
```

访问 <http://127.0.0.1:18080>，启用认证时使用上述凭据登录，在 OpenVPN 页面导入
`client.ovpn`、填写证书和凭据并连接。默认手动 DNS 为
`1.1.1.1`，请按 VPN 网络要求调整，或选择使用 OpenVPN 下发 DNS。

HTTP 代理默认开启且无认证，地址为 `127.0.0.1:17888`；SOCKS5 和 SSH 默认关闭。示例端口仅绑定宿主机
`127.0.0.1`，配置持久化到 `./config`。管理用户名或密码修改后，执行
`docker compose up -d --force-recreate openvpn-web` 重新创建容器。

Docker Run、端口映射、局域网访问和 GHCR 镜像用法见[部署与启动](docs/deployment.md)。

## 文档

| 文档 | 内容 |
| --- | --- |
| [部署与启动](docs/deployment.md) | 环境要求、Compose / Docker Run、管理认证、端口映射、GHCR 镜像 |
| [配置与服务](docs/configuration.md) | 多套 VPN 配置、OpenVPN、HTTP/SOCKS5、SSH、日志、endpoint 自动选择 |
| [DNS 与网络](docs/networking.md) | HTTPDNS、DNS 热更新、网络工具、出口保护、OpenVPN 参数边界 |
| [健康检查](docs/health-checks.md) | 容器与 VPN 健康、探测参数、失败阈值、自动恢复 |
| [存储与权限](docs/storage.md) | 持久化目录、密钥与账户权限、备份、日志命令、YAML 结构 |
| [开发与验证](docs/development.md) | 技术栈版本、本地前端与容器后端、验证命令、GitHub CI |
