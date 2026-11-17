# 项目协作指南

本文件适用于整个仓库。默认使用简体中文与用户沟通，并编写执行说明、计划、TODO、任务文档及澄清选项；用户或交付物明确要求其他语言时按要求执行。

## 项目与目录

项目通过单个 Docker 容器提供 OpenVPN、HTTP/SOCKS5 代理、可选 SSH 和 Web 管理界面。后端及运行时逻辑使用 Rust/Axum/Tokio，前端使用 React/TypeScript/Vite，容器内由 Supervisor 管理服务。Python 仅用于 Supervisor 依赖和开发/CI 的 smoke 脚本，不承担产品业务逻辑。

| 路径 | 职责 |
| --- | --- |
| `src/main.rs` | Axum 路由、管理认证、同源校验、API 和程序入口 |
| `src/config.rs` | 配置模型、默认值、固定端口和输入校验 |
| `src/storage.rs` | YAML 持久化、证书与私钥提取、文件权限 |
| `src/runtime.rs`、`src/runtime/health.rs` | 服务配置与控制、防火墙、选点、DNS、账户隔离和健康恢复 |
| `src/network.rs`、`src/tools.rs`、`src/tools/path_trace.rs` | HTTPDNS、网络探测、MTR/Traceroute WebSocket |
| `frontend/src/main.tsx` | 前端页面、Hash 路由和交互 |
| `frontend/src/api.ts`、`frontend/src/types.ts` | API 请求及前端类型；修改接口时同步核对 |
| `docker/`、`Dockerfile`、`compose.yaml` | 启动脚本、Supervisor 配置、多阶段构建与部署 |
| `scripts/dev.sh`、`compose.dev.yaml` | tmux 开发环境、临时后端、Rust 构建与进程更新 |
| `scripts/smoke.py` | 使用隔离 Docker 网络和临时 VPN 服务端的集成验证 |
| `docs/` | 部署、配置、网络、健康检查、存储和开发文档 |

根目录 `README.md` 保持简洁，只保留简介、快速开始和文档导航；详细说明放入相应的 `docs/` 文档，移动内容时同步修正相对链接和标题锚点。

## 开发环境与常用命令

Rust 工具链以 `rust-toolchain.toml` 为准，Node 版本以 `frontend/.node-version` 为准，pnpm 版本以 `frontend/package.json` 的 `packageManager` 为准。使用 pnpm 管理前端依赖，保留并按需更新 `Cargo.lock` 和 `frontend/pnpm-lock.yaml`。

推荐本机运行 Vite、容器运行后端和 VPN 服务，具体步骤见 [开发与验证](docs/development.md)。首次启动时复制 `.env.example` 为 `.env`，已有文件则保留原配置，设置 `ADMIN_USERNAME` 和 `ADMIN_PASSWORD` 后启动。以下命令在仓库根目录执行：

```bash
./scripts/dev.sh start
./scripts/dev.sh stop
./scripts/dev.sh restart
```

开发环境使用一个 `openvpn-web-dev` tmux 会话：`frontend` 窗口运行本机 pnpm / Vite，`backend` 窗口构建并通过 `docker compose run --rm --service-ports` 运行临时后端、显示容器日志。停止时删除临时容器，保留配置与构建缓存；已有普通 Compose 后端须先自行停止。

前端默认地址为 `http://127.0.0.1:5173`，后端默认地址为 `http://127.0.0.1:18080`。Vite 从根目录 `.env` 读取 `WEB_PORT`，自动代理 `/api` 和 `/healthz`，保留原始 `Host` 并支持 WebSocket，以满足后端同源校验；前端开发请求应始终经该代理访问容器后端。

修改 Rust 代码后执行 `./scripts/dev.sh build`：脚本在匹配运行环境的 Rust 容器中增量构建，成功后原子替换目录挂载的 `target/dev-dist/openvpn-web`，自动重启 Supervisor 的 `backend`、`health` 和当时已运行的 `socks5`；构建失败保留当前版本，无需 `docker cp`。不重跑一次性 bootstrap，不开启已停止的可选服务。涉及启动、防火墙初始化、Docker 或 `.env` 配置的变更执行 `./scripts/dev.sh restart`。

开发 Supervisor 提供 HTTP / XML-RPC 管理，默认 `http://127.0.0.1:19001`，使用 `.env` 的管理账号，支持网页直接重启进程；`SUPERVISOR_PORT` 调整宿主端口。此网络管理入口仅开发镜像启用，生产仍使用 Unix socket。

按变更范围执行现有检查，与 [GitHub CI](.github/workflows/docker.yml) 保持一致：

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend build
docker compose config --quiet
```

涉及镜像、VPN、出口保护、服务认证或持久化等运行行为时，按需构建镜像并执行集成验证：

```bash
docker build -t openvpn-web:local .
python3 scripts/smoke.py --image openvpn-web:local
```

smoke 脚本需要 Linux Docker Engine、`/dev/net/tun`、内核 nftables 支持，以及宿主机的 Python 3、`openssl`、`ssh` 和 `curl`；脚本自行创建并清理测试容器、网络和临时数据。现有 Rust 单元测试位于相应模块的 `#[cfg(test)]` 中，前端 `build` 同时执行 TypeScript 检查与生产构建。

## 设计与实现约定

- 接口只承载当前调用方实际需要的能力，优先复用现有约定；不为假设的未来需求增加接口层、多层包装、通用框架或大量可选参数。
- 保持职责清晰、低耦合和可组合，出现真实需求再提取抽象，不提前堆砌扩展点，也不过度限制合理扩展。
- 校验集中在信任边界，依据明确契约、真实输入和已知故障；内部遵循已确认的不变量，不添加无依据的重复校验、防御分支或静默兜底，保留必要的权限和数据完整性检查。
- 新增代码的注释优先使用简体中文，解释意图、边界、不变量、权限或调用契约。修改已有文件时与周围注释语言一致，不混入另一种语言或改写无关注释。
- 配置或接口变更需同步核对 Rust 模型、存储格式、前端类型与相关文档；兼容性处理沿用现有约定。

## 网络、权限与数据约束

- 使用独立 Docker bridge 网络，不支持 host 模式或共享其他容器的网络命名空间。固定使用 `tun0`、IPv4 VPN 远端和全隧道路由。
- 保持服务启动前安装默认拒绝的 nftables 策略、失败即退出的行为。VPN 断线后，代理和 SSH 的新出站连接不能回退到 Docker 默认网关；隧道外 IPv6 同样默认拒绝。隧道外例外规则详见 [DNS 与网络](docs/networking.md)。
- 容器端口固定为 Web `80`、HTTP `8888`、SOCKS5 `1080`、SSH `22`。开发镜像额外启用 Supervisor `9001`，仅映射宿主机回环地址。宿主机映射通过部署配置调整，并同步对应的 `PUBLISHED_*` 展示值，不重新引入各 profile 的监听端口配置。
- 同一时间只运行一套 VPN 配置。切换 profile 时保持旧 SSH 会话终止、服务切换和账户隔离；“保存配置”与应用或重启服务的语义保持一致。
- 管理认证由 `ADMIN_USERNAME`、`ADMIN_PASSWORD` 环境变量配置；保持写请求和 WebSocket 的同源校验。`/healthz` 不要求认证，不应返回配置或密钥。
- `/config` 保存真实配置、密钥、日志及 SSH 数据，`/run/vpnweb` 保存运行时文件。保持配置和密钥文件 `0600`、密钥目录 `0700` 及现有账户隔离规则；存储结构见 [存储与权限](docs/storage.md)。
- 不提交 `.env`、真实 VPN 配置、证书、私钥、密码、日志或 `config/` 数据。操作已有开发容器前注意其挂载的真实配置，避免覆盖或清理用户数据。

## 测试与验证约定

- 按风险选择验证方式，优先复用现有测试；简单、可逆、低影响的修改不新增测试。有明确测试要求或回归风险时补充最小用例集。
- 同时涉及实现和单元测试时，先完成实现，再运行回归测试，按实际结果和需要调整测试，不默认先修改测试。
- 测试聚焦对外可观察的行为、修复的缺陷和有依据的关键边界；不为覆盖率穷举假设场景，不断言内部调用顺序或私有状态。
- mock、fixture 与场景相称，不为少量用例搭建通用框架；断言只验证有意义的结果和必要副作用，不重复验证同一事实或复刻实现逻辑。
- 纯文档变更检查内容、链接、锚点和格式即可。交付时说明实际执行的验证及尚未验证的内容。

## Git 与本机操作

- 默认在当前分支工作，保留已有未提交变更。commit、push 和发布遵循用户指令或已有授权；commit 信息禁止添加 `Co-Authored-By` 行。
- 用户未明确要求或授权新建分支时，先询问“新建分支 / 继续当前分支”，收到选择后再执行；优先使用适用的结构化提问工具。用户已授权时无需重复确认。
- 新建 worktree 放在项目同级的 `<项目名>-worktrees/<worktree名>/`，本项目对应 `../openvpn-web-docker-worktrees/<worktree名>/`；不放入项目内部。若同时新建分支，仍遵循上述确认规则。
- 本机开发禁止使用 Rootless Docker；远程生产或 DEV 服务允许使用 Rootless Docker。需要 MySQL 或 PostgreSQL 时复用本地已有实例，不新建对应数据库容器。
- 使用 `sudo` 前先执行 `sudo -n -l` 查询免密命令范围，再按授权执行；不直接读取 sudoers 文件判断权限。
- 本地浏览器调试优先查找 chrome-for-testing 对应的 Chrome DevTools，找不到时再使用 Chromium 等其他浏览器。
