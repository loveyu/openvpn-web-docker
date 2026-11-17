# 部署与启动

[返回项目首页](../README.md) · 本文命令默认在仓库根目录执行。

镜像提供 `linux/amd64` 和 `linux/arm64`，Docker 会按引擎架构自动选择：

| 宿主机              | Docker 环境                  | 镜像平台      |
| ------------------- | ---------------------------- | ------------- |
| Linux x86-64        | Docker Engine                | `linux/amd64` |
| Linux ARM64         | Docker Engine                | `linux/arm64` |
| macOS Intel         | Docker Desktop（Linux 容器） | `linux/amd64` |
| macOS Apple Silicon | Docker Desktop（Linux 容器） | `linux/arm64` |

需要 Docker 引擎所在 Linux 环境提供 `/dev/net/tun` 和内核 nftables 支持；采用 Docker Compose 时还需 Compose 插件。
macOS 先启动 Docker Desktop，后续 Compose 和 Docker Run 命令与 Linux 相同。`--device /dev/net/tun` 映射的是 Docker Desktop Linux 虚拟机中的设备，macOS 宿主机无需创建该设备，也无需把 `platform` 固定为 `linux/amd64`。

使用独立
**bridge 网络，不支持 host 模式或共享其他容器的网络命名空间**；防火墙与路由只作用于本容器。

以下两种启动方式的示例默认配置管理用户名和密码，示例中的密码请替换为自己的强密码。管理认证可选，无需认证时将 `ADMIN_USERNAME`、`ADMIN_PASSWORD` 同时留空，或在 Docker Run 中省略这两个环境变量。

## Docker Compose（推荐）

首次使用时复制环境变量模板；已有 `.env` 时直接编辑，保留原有配置：

```bash
test -f .env || cp .env.example .env
```

在项目根目录的 `.env` 中设置：

```dotenv
ADMIN_USERNAME=admin
ADMIN_PASSWORD='请替换为强密码'
```

然后启动：

```bash
docker compose up -d --build
```

修改用户名或密码后，执行 `docker compose up -d --force-recreate openvpn-web` 重新创建容器；仅执行
`docker compose restart` 不会更新环境变量。

## Docker Run

先构建本地镜像，然后通过 `-e` 传入管理用户名和密码：

```bash
docker build -t openvpn-web:local .
docker run -d --name openvpn-web --restart unless-stopped \
  --cap-add NET_ADMIN --device /dev/net/tun:/dev/net/tun \
  --sysctl net.ipv4.conf.all.src_valid_mark=1 \
  --sysctl net.ipv4.conf.all.rp_filter=2 \
  --tmpfs /run \
  -v "$(pwd)/config:/config" \
  -p 127.0.0.1:18080:80 \
  -p 127.0.0.1:17888:8888 \
  -p 127.0.0.1:17080:1080 \
  -p 127.0.0.1:17222:22 \
  -e ADMIN_USERNAME=admin \
  -e ADMIN_PASSWORD='请替换为强密码' \
  -e TZ=Asia/Shanghai \
  -e PUBLISHED_WEB_PORT=18080 \
  -e PUBLISHED_HTTP_PORT=17888 \
  -e PUBLISHED_SOCKS5_PORT=17080 \
  -e PUBLISHED_SSH_PORT=17222 \
  openvpn-web:local
```

两种方式任选其一。Docker Run 修改环境变量后也需要重新创建容器，挂载的 `config` 目录保留配置。

访问 <http://127.0.0.1:18080>。在 OpenVPN 页面粘贴或导入
`client.ovpn`，填写证书、私钥及所需凭据，配置 endpoint 后点击“保存并连接”。默认手动 DNS 为
`1.1.1.1`，请按 VPN 网络要求修改，也可切换到“使用 OpenVPN 下发 DNS”。SOCKS5 和 SSH 默认关闭，在各自页面启用。

| 服务                 | 默认状态                    | 容器端口 | 默认宿主机端口 |
| -------------------- | --------------------------- | -------- | -------------- |
| 管理页面 / API       | 开启，按示例配置 Basic Auth | 80       | 18080          |
| HTTP / HTTPS CONNECT | 开启，无认证                | 8888     | 17888          |
| SOCKS5 TCP CONNECT   | 关闭                        | 1080     | 17080          |
| SSH / SFTP / 转发    | 关闭                        | 22       | 17222          |

示例默认绑定宿主机 `127.0.0.1`。局域网访问时，Compose 设置 `.env` 的 `BIND_ADDRESS`，Docker Run 修改
`-p` 的绑定地址。管理 Basic Auth 通过 `ADMIN_USERNAME`、`ADMIN_PASSWORD`
环境变量设置，两项同时留空时关闭认证，启用时两项同时提供，修改后重新创建容器。启用认证后浏览器使用原生认证对话框，API 使用
`curl -u '用户名:密码'`；点击页面中的 Basic Auth 可查看设置说明。`/healthz`
供容器健康检查使用，不要求认证，不包含配置或密钥。

## 端口映射与 Docker 网络

容器监听端口固定为 Web `80`、HTTP 代理 `8888`、SOCKS5 `1080`、SSH `22`。后台不提供端口调整，所有 VPN 配置使用相同监听端口。

读取旧版 YAML 时会忽略已废弃的 `web` 和各服务 `port`
字段，下一次保存配置时移除这些字段；API 不再接受服务监听端口设置。

宿主机端口由 Docker 映射决定。在 `.env` 设置
`WEB_PORT`、`HTTP_PROXY_PORT`、`SOCKS5_PORT`、`SSH_PORT`，或直接修改 `compose.yaml`
映射左侧；右侧保持容器固定端口。例如 Web 使用 `18080:80`，SSH 使用 `17222:22`。调整映射后执行
`docker compose up -d --force-recreate`。

概览按服务及其子进程检测实际 TCP 监听端口，未监听时显示服务预设端口。Compose 使用同一组端口变量向后端传入
`PUBLISHED_WEB_PORT`、`PUBLISHED_HTTP_PORT`、`PUBLISHED_SOCKS5_PORT`、`PUBLISHED_SSH_PORT`，页面同时显示“宿主机端口 → 容器目标端口”。这些映射信息来自部署配置，容器不读取 Docker API。直接修改 Compose 的映射或使用 `docker run` 时，也应同步提供对应的 `PUBLISHED_*`
值；未提供时页面明确显示“宿主机映射未提供”。

**不要设置
`network_mode: host`**。当前实现依赖独立网络命名空间、IPv4 默认网关、连接标记和策略路由；也不提供其他容器通过 IP 转发共享隧道的网关模式。

## 使用 GHCR 镜像

首次成功发布后可在 `.env` 设置 `IMAGE=ghcr.io/<owner>/<repo>:latest`，执行 `docker compose pull` 和
`docker compose up -d --no-build`。

同一标签同时包含 AMD64 和 ARM64 镜像，适用于上述 Linux 和 macOS 环境，无需选择架构专用标签。可检查镜像支持的平台：

```bash
docker buildx imagetools inspect ghcr.io/<owner>/<repo>:latest
```

镜像构建、发布触发条件与仓库权限见[开发与验证](development.md#github-ci)。
