# 开发与验证

[返回项目首页](../README.md) · 本文命令默认在仓库根目录执行。

## 版本与依赖

自有 API、配置生成、网络探测、防火墙、DNS hook 和健康检查全部使用 Rust。Python 仅作为 Supervisor 及其 supervisorctl 的依赖；镜像没有自有 Python 应用。OpenVPN、Tinyproxy、Dante 和 OpenSSH 直接集成到镜像中。

| 层级 | 版本 / 选型 |
| --- | --- |
| 后端 | Rust 1.100.0、Axum 0.8.9、Tokio 1.54.1 |
| 前端 | React 19.3.1、TypeScript 7.0.2、Vite 8.3.8 |
| 构建 Node | 26.13.1 LTS |
| 前端包管理 | pnpm 12.11.2 |
| 底包 | Debian 13 slim（trixie） |
| 守护 | Supervisor；Python 仅为其依赖 |
| 应用 | Debian 13 仓库的 OpenVPN、Tinyproxy、OpenSSH；官方源码 Dante 1.4.4 |

依赖使用 `Cargo.lock` 和 `frontend/pnpm-lock.yaml` 固定版本。前端通过 `.node-version` 指定 Node 版本，通过 `package.json` 的 `packageManager` 固定 pnpm 版本；本地、Docker 和 CI 均使用 pnpm 安装与构建。系统程序使用 Debian 13 稳定仓库更新，不混用 unstable。Dante 因不在 Debian 13 仓库中，从官方源码校验 SHA256 后构建。

## GitHub CI

[GitHub 工作流](../.github/workflows/docker.yml) 在 PR、main、`v*` 标签、手动和每周触发：

1. Rust fmt / Clippy / 单元测试，前端类型检查和生产构建。
2. AMD64 与 ARM64 原生 runner 分别构建完整镜像，执行隔离 Docker 集成测试。
3. 非 PR 运行在测试通过后发布 GHCR 多架构 manifest（latest、版本、提交 SHA）。

仓库需允许 Actions 写入 Packages；镜像公开性由 GHCR 仓库设置控制。已发布镜像的使用方式见[部署与启动](deployment.md#使用-ghcr-镜像)。本地不自动推送或发布。

## 本地前端 + 容器后端

前端使用本机 Node.js 和 pnpm 运行 Vite，推荐通过 [fnm](https://github.com/Schniz/fnm) 管理 Node 版本。`frontend/.node-version` 固定为 `26.13.1`，与 Docker、CI 一致。Rust 后端和 OpenVPN、代理、Supervisor 等依赖运行在 Docker 容器内；本机无需安装 Rust 即可启动这套开发环境。

终端一，按[部署说明](deployment.md#docker-compose推荐)配置管理认证，并在仓库根目录启动后端：

```bash
# 仅首次创建；已有 .env 时保留原配置
test -f .env || cp .env.example .env
# 先在 .env 中设置 ADMIN_USERNAME 和 ADMIN_PASSWORD
docker compose up -d --build
docker compose logs -f --tail=100 openvpn-web
```

终端二，安装 fnm 后，在 Bash 中启动本地前端：

```bash
cd frontend
eval "$(fnm env --use-on-cd --shell bash)"
fnm use --install-if-missing
# 此 Node 版本首次使用时安装 pnpm；已安装对应版本则跳过
npm install --global pnpm@12.11.2
pnpm install --frozen-lockfile
pnpm dev
```

可将 `eval "$(fnm env --use-on-cd --shell bash)"` 加入 `~/.bashrc`，此后进入 `frontend` 目录时会自动切换 Node。增删依赖使用 `pnpm add` / `pnpm remove`，并提交更新后的 `pnpm-lock.yaml`。

访问 Vite 输出的本地地址，默认是 <http://127.0.0.1:5173>。使用 `.env` 中设置的管理用户名和密码登录。前端代码修改后热更新；`/api` 和 `/healthz` 由 Vite 转发到 `http://127.0.0.1:${WEB_PORT}`，其中 `WEB_PORT` 读取仓库根目录的 `.env`，默认 `18080`。代理支持 WebSocket 并保留原始 `Host`，以通过后端写操作和 WebSocket 的同源校验。修改 `WEB_PORT` 后，需要重新创建容器并重启 Vite。

修改 Rust 代码或容器配置后，在仓库根目录执行以下命令重新构建并更新后端：

```bash
docker compose up -d --build openvpn-web
```

VPN、代理和服务操作会作用于容器及其挂载的 `./config`，开发前按需备份该目录。

## 验证

以下 Rust 静态检查和单元测试需要本机安装 `rust-toolchain.toml` 指定的工具链：

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cd frontend
fnm use --install-if-missing
pnpm install --frozen-lockfile
pnpm build
cd ..
docker build -t openvpn-web:local .
python3 scripts/smoke.py --image openvpn-web:local
docker compose config --quiet
```

Python smoke 脚本仅在开发机/CI 运行，不复制进镜像。它创建独立 Docker bridge、临时 TLS VPN 服务端、DNS/HTTP 测试夹具和客户端，验证 UDP 自动选点、HTTPDNS、DNS 热更新、代理与 SSH 认证、密钥权限、固定容器端口和宿主机映射、多配置认证隔离、健康检查、服务重启、断线保护和持久化；结束后清理自己的容器和网络。需要宿主机 `openssl`、`ssh`、`curl`、Python 3 和 Docker。
