# 开发与验证

[返回项目首页](../README.md) · 本文命令默认在仓库根目录执行。

## 版本与依赖

自有 API、配置生成、网络探测、防火墙、DNS hook 和健康检查全部使用 Rust。Python 仅作为 Supervisor 及其 supervisorctl 的依赖；镜像没有自有 Python 应用。OpenVPN、Tinyproxy、Dante 和 OpenSSH 直接集成到镜像中。

| 层级       | 版本 / 选型                                                        |
| ---------- | ------------------------------------------------------------------ |
| 后端       | Rust 1.100.0、Axum 0.8.9、Tokio 1.54.1                             |
| 前端       | React 19.3.1、TypeScript 7.0.2、Vite 8.3.8                         |
| 构建 Node  | 26.13.1 LTS                                                        |
| 前端包管理 | pnpm 12.11.2                                                       |
| 底包       | Debian 13 slim（trixie）                                           |
| 守护       | Supervisor；Python 仅为其依赖                                      |
| 应用       | Debian 13 仓库的 OpenVPN、Tinyproxy、OpenSSH；官方源码 Dante 1.4.4 |

依赖使用 `Cargo.lock` 和 `frontend/pnpm-lock.yaml` 固定版本。前端通过 `.node-version`
指定 Node 版本，通过 `package.json` 的 `packageManager`
固定 pnpm 版本；本地、Docker 和 CI 均使用 pnpm 安装与构建。系统程序使用 Debian 13 稳定仓库更新，不混用 unstable。Dante 因不在 Debian 13 仓库中，从官方源码校验 SHA256 后构建。

## GitHub CI

[GitHub 工作流](../.github/workflows/docker.yml) 在 PR、main、`v*` 标签、手动和每周触发：

1. Rust、TypeScript、Python、Shell、YAML、TOML、Dockerfile 和 Compose 的零 warning 质量检查；前端在此生成一次 dist，作为 artifact 供后续镜像构建复用。
2. AMD64 与 ARM64 原生 runner 分别构建完整镜像，执行隔离 Docker 集成测试；两者通过 `frontend-assets` 命名构建上下文使用同一份 dist，不重复安装或编译前端。
3. 非 PR 运行在测试通过后发布 GHCR 多架构 manifest（latest、版本、提交 SHA），并检查包含 `linux/amd64` 与 `linux/arm64`。

这两种 Linux 镜像覆盖 Linux 主机及使用 Docker Desktop 的 Intel / Apple Silicon Mac。CI 在 Linux 原生 runner 上验证容器行为；macOS Docker Desktop 仍需在对应环境实机验证。

仓库需允许 Actions 写入 Packages；镜像公开性由 GHCR 仓库设置控制。已发布镜像的使用方式见[部署与启动](deployment.md#使用-ghcr-镜像)。本地不自动推送或发布。

CI 缓存 Rust 检查所需的 Cargo 依赖与编译产物、pnpm 依赖，以及按 AMD64 / ARM64 分开的 BuildKit 镜像层（`type=gha,mode=max`）。运行时系统包仍通过 `no-cache-filters: runtime` 刷新；前端 dist 在同一次工作流内通过 artifact 共享。

## 本地镜像构建

本地只构建当前 Docker 引擎的平台和架构，使用以下任一命令即可，无需指定 `--platform`：

```bash
docker build -t openvpn-web:local .
docker compose up -d --build
```

Apple Silicon 的 Docker Desktop 默认构建 `linux/arm64`，Intel Mac 和 x86-64 Linux 默认构建 `linux/amd64`。本地前端静态资源在 Docker 内使用构建机架构生成，Rust 后端和 Dante 按目标架构编译，Rust 编译产物缓存按架构隔离。多架构构建、集成测试与发布由 GitHub CI 完成，本地无需配置 QEMU 或构建其他架构。

## 本地前端 + 容器后端

开发入口为 `./scripts/dev.sh`，需要 Bash、tmux、普通 Docker Engine / Docker Desktop、Docker Compose、Node.js 和 pnpm。本机无需安装 Rust。前端使用本机 Node.js 运行 Vite；Rust 在与运行镜像相同的 Debian 13、相同架构的 Rust 容器中编译，VPN 和代理运行在开发后端容器内。

推荐使用 [fnm](https://github.com/Schniz/fnm) 管理 Node 版本，首次配置：

```bash
eval "$(fnm env --use-on-cd --shell bash)"
cd frontend
fnm use --install-if-missing
npm install --global pnpm@12.11.2
cd ..
# 仅首次创建；已有 .env 时保留原配置
test -f .env || cp .env.example .env
# 管理认证可选；需要时在 .env 中同时设置 ADMIN_USERNAME 和 ADMIN_PASSWORD
```

启动、停止和重启：

```bash
./scripts/dev.sh start
./scripts/dev.sh stop
./scripts/dev.sh restart
```

不带参数等同于 `start`。脚本创建一个 `openvpn-web-dev` tmux 会话，包含两个窗口：

| 窗口 | 行为 |
| --- | --- |
| `frontend` | 自动执行 `pnpm install --frozen-lockfile`，再运行 `pnpm dev --port "$FRONTEND_PORT" --strictPort` |
| `backend` | 构建开发镜像和 Rust，使用 `docker compose run --detach --rm --service-ports` 启动 `openvpn-web-dev` 容器，并持续显示容器日志 |

启动前检查前端及所有容器映射端口；发现占用时显示端口、占用进程或容器，以及查看和调整端口的命令，并返回失败。前端端口通过根目录 `.env` 的 `FRONTEND_PORT` 设置，默认 `5173`；也可临时执行 `FRONTEND_PORT=5174 ./scripts/dev.sh start`。后端端口按提示调整 `.env` 对应变量。

脚本等待前端、后端健康检查和 Supervisor 就绪后，显示实际前端、后端健康检查和 Supervisor URL，再进入 tmux；已有会话时重复 `start` 同样确认就绪并显示 URL。启动失败时提示对应窗口，错误输出保留在窗口中。首次构建可能较慢，等待期间可在另一终端查看 tmux 输出。

交互终端会进入该会话；在 tmux 内执行会切换会话。按 `Ctrl-b d` 离开后服务继续运行，用 `tmux attach-session -t openvpn-web-dev` 重新进入；按 `Ctrl-b w` 选择窗口。停止命令终止两个窗口、构建容器和开发后端，后端退出后由 `--rm` 自动删除；`./config` 和 `target/` 下的构建缓存保留。

已有普通 Compose 后端运行时，脚本会提示先自行停止，避免共用配置和端口；脚本不会自动停止该容器。VPN、代理和服务操作会作用于挂载的 `./config`，开发前按需备份。

访问 <http://127.0.0.1:5173>；`.env` 中的 `ADMIN_USERNAME` 和 `ADMIN_PASSWORD` 同时留空时无需登录，配置了两项时使用该账号登录。认证是可选项，脚本不强制配置；缺少 `.env` 时自动复制默认关闭认证的 `.env.example` 并继续启动。前端代码修改后热更新；`/api` 和 `/healthz` 由 Vite 自动转发到 `http://127.0.0.1:${WEB_PORT}`，`WEB_PORT` 读取根目录 `.env`，默认 `18080`。代理支持 WebSocket 并保留原始 `Host`，满足后端同源校验。

### 更新 Rust 代码

修改 Rust 代码后，在另一终端执行：

```bash
./scripts/dev.sh build
```

脚本在临时 Rust 容器中增量执行 debug 构建；成功后将二进制原子替换到 `target/dev-dist/openvpn-web`，再通过 `supervisorctl` 重启 `backend`、`health` 和当时已运行的 `socks5`。编译失败不会替换产物或重启进程；不会重跑一次性的 `bootstrap`，也不会开启已停止的可选服务。重启 SOCKS5 会中断已有 SOCKS5 连接。

整个 `target/dev-dist` 目录只读挂载到容器 `/app/dev`，开发镜像中的 `/usr/local/bin/openvpn-web` 链接到该目录，无需 `docker cp`。使用目录挂载使原子替换后的新文件立即可见；直接挂载单个二进制文件会固定旧文件的 inode。Rust 构建缓存和 Cargo 下载缓存分别保存在 `target/dev-build`、`target/dev-cargo`，以宿主用户身份写入。

修改 `.env`、`Dockerfile`、`docker/` 或 Compose 配置后执行 `./scripts/dev.sh restart`，重新构建并创建开发容器。涉及防火墙初始化或启动流程的 Rust 变更也应使用 `restart`，以验证完整启动过程。

### Supervisor 网络管理

仅开发镜像启用 Supervisor HTTP / XML-RPC，宿主机固定绑定 `127.0.0.1`，默认地址为 <http://127.0.0.1:19001>，端口可在 `.env` 中通过 `SUPERVISOR_PORT` 调整。认证与后端使用同一组 `ADMIN_USERNAME`、`ADMIN_PASSWORD`，两项同时留空时可直接访问；可在网页直接查看、停止或重启进程，也可用支持 XML-RPC 的客户端连接 `/RPC2`。开发镜像设置 `VPNWEB_DEV=1`，防火墙据此允许容器端口 `9001` 的入站管理连接及回包，保持隧道外主动出站默认拒绝。生产镜像不设置该标记，仅保留原有 Unix socket。

手动重启进程不会编译 Rust；Rust 更新统一使用 `./scripts/dev.sh build`，确保新产物已经发布。

## API 与 AI 调试

后端直接生成两份纯文本 API 文档，无需部署独立的 txt 文件：

| 地址 | 内容 |
| --- | --- |
| `/llms.txt` | 接口索引、认证约定和调试顺序 |
| `/llms-full.txt` | 全部接口的参数与响应、从 Rust 默认模型生成的配置示例、WebSocket 协议、错误处理和调用示例 |

`/llm.txt`、`/llm-full.txt` 分别以 HTTP **302** 跳转到上表对应地址。启用管理认证时，文档读取和跳转均需携带同一组 Basic Auth 凭据。文档不读取当前实例配置或密钥。

页面 `<head>` 通过两个 `rel="alternate"` 链接声明文档地址，所有页面的页脚也提供入口。Vite 自动代理这四个地址，可从默认的 <http://127.0.0.1:5173/llms.txt> 和 <http://127.0.0.1:5173/llms-full.txt> 访问；后端默认使用 `18080` 端口。

简版与完整版共用 [后端接口描述](../src/api_docs.rs)。修改 API 时同步更新描述；配置默认示例随 Rust 模型生成。

## 质量检查与格式化

仓库提供两个统一脚本。`format.sh` 会修改源代码、配置和文档；`check.sh` 执行检查，任何
warning、格式差异、类型错误、lint 错误或构建失败都会返回非零状态：

```bash
pnpm --dir frontend install --frozen-lockfile
./scripts/format.sh
./scripts/check.sh
```

脚本覆盖 Rust (`rustfmt`、Clippy、单元测试)、TypeScript/React (TypeScript、Oxlint、Prettier)、
Python (Ruff)、POSIX Shell (shfmt、ShellCheck)、YAML (yamllint)、TOML (taplo)、Dockerfile (Hadolint)、SVG XML 语法和 Docker Compose。`scripts/check.sh` 不会自动安装依赖；除 Rust、Node.js
和 pnpm 外，还需要在 PATH 中提供 Ruff、shfmt、ShellCheck、yamllint、taplo、hadolint 与 Docker。
GitHub Actions 会固定这些工具的版本。

## 集成验证

质量检查通过后，再构建镜像并运行 smoke 测试：

```bash
docker build -t openvpn-web:local .
python3 scripts/smoke.py --image openvpn-web:local
docker compose config --quiet
```

Python smoke 脚本仅在开发机/CI 运行，不复制进镜像。它创建独立 Docker bridge、临时 TLS VPN 服务端、DNS/HTTP 测试夹具和客户端，验证 UDP 自动选点、HTTPDNS、DNS 热更新、代理与 SSH 认证、密钥权限、固定容器端口和宿主机映射、多配置认证隔离、健康检查、服务重启、断线保护和持久化；结束后清理自己的容器和网络。需要宿主机
`openssl`、`ssh`、`curl`、Python 3 和 Docker。
