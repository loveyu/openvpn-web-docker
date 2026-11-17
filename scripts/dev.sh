#!/usr/bin/env bash
# 一个 tmux 会话运行本机前端与临时后端容器；Rust 产物通过目录挂载更新。
set -Eeuo pipefail

ROOT_DIR=$(cd -- "$(dirname -- "$0")/.." && pwd)
SESSION=openvpn-web-dev
CONTAINER=$SESSION
BUILD_CONTAINER=$SESSION-build
SUPERVISOR_CONFIG=/etc/supervisor/supervisord.conf
cd "$ROOT_DIR"

compose() {
    docker compose -f "$ROOT_DIR/compose.yaml" -f "$ROOT_DIR/compose.dev.yaml" "$@"
}

require_command() {
    if ! command -v "$1" > /dev/null 2>&1; then
        echo "缺少开发工具：$1" >&2
        exit 127
    fi
}

check_docker() {
    local security_options
    security_options=$(docker info --format '{{json .SecurityOptions}}')
    if [[ "$security_options" == *rootless* ]]; then
        echo "本机开发请使用普通 Docker Engine，不支持 Rootless Docker。" >&2
        exit 1
    fi
}

build_backend() {
    local rust_version
    rust_version=$(sed -n 's/^channel = "\([^"]*\)"/\1/p' rust-toolchain.toml)
    mkdir -p target/dev-cargo target/dev-build target/dev-dist
    # 构建镜像与运行镜像使用相同的 Debian 和架构；以宿主用户写入缓存和产物。
    docker run --rm --name "$BUILD_CONTAINER" \
        --user "$(id -u):$(id -g)" \
        --env CARGO_HOME=/workspace/target/dev-cargo \
        --volume "$ROOT_DIR:/workspace" --workdir /workspace \
        "rust:$rust_version-slim-trixie" \
        cargo "+$rust_version" build --locked --target-dir target/dev-build
    # 容器挂载整个目录，原子替换不会覆盖正在执行的文件或暴露未写完的产物。
    install -m 755 target/dev-build/debug/openvpn-web target/dev-dist/openvpn-web.next
    mv -f target/dev-dist/openvpn-web.next target/dev-dist/openvpn-web
}

reload_backend() {
    local socks_pid
    local programs=(backend health)
    # Supervisor 对停止的进程输出 0，同时返回非零状态。
    if ! socks_pid=$(docker exec "$CONTAINER" supervisorctl -c "$SUPERVISOR_CONFIG" pid socks5) && [[ "$socks_pid" != 0 ]]; then
        echo "读取 SOCKS5 进程状态失败：$socks_pid" >&2
        return 1
    fi
    if [[ "$socks_pid" != 0 ]]; then
        programs+=(socks5)
    fi
    # 一次性 bootstrap 不重跑；已停止的可选服务不主动开启。
    docker exec "$CONTAINER" supervisorctl -c "$SUPERVISOR_CONFIG" restart "${programs[@]}"
}

enter_session() {
    if [[ -t 0 && -t 1 ]]; then
        if [[ -n "${TMUX:-}" ]]; then
            tmux switch-client -t "=$SESSION"
        else
            exec tmux attach-session -t "=$SESSION"
        fi
    else
        echo "查看输出：tmux attach-session -t $SESSION"
    fi
}

start() {
    if tmux has-session -t "=$SESSION" 2> /dev/null; then
        FRONTEND_PORT=$(tmux show-environment -t "=$SESSION" FRONTEND_PORT)
        FRONTEND_PORT=${FRONTEND_PORT#FRONTEND_PORT=}
        compose config --format json | node "$ROOT_DIR/scripts/dev.mjs" wait "$FRONTEND_PORT"
        enter_session
        return
    fi
    require_command node
    require_command pnpm
    check_docker
    if [[ ! -f .env ]]; then
        cp .env.example .env
        echo "已创建 .env。"
    fi
    compose config --quiet
    FRONTEND_PORT=$(compose config --environment | sed -n 's/^FRONTEND_PORT=//p')
    FRONTEND_PORT=${FRONTEND_PORT:-5173}
    compose config --format json | node "$ROOT_DIR/scripts/dev.mjs" check "$FRONTEND_PORT"
    if [[ -n "$(compose ps --status running --quiet openvpn-web)" ]]; then
        echo "已有 Compose 后端在运行，请先执行 docker compose stop openvpn-web。" >&2
        exit 1
    fi
    # 使用当前 PATH，避免已有 tmux 服务仍使用旧的 Node/pnpm 环境。
    tmux new-session -d -s "$SESSION" -n backend -c "$ROOT_DIR" -e "PATH=$PATH" -e "FRONTEND_PORT=$FRONTEND_PORT" \
        "$BASH" "$ROOT_DIR/scripts/dev.sh" _backend \; \
        set-option -w -t "$SESSION:backend" remain-on-exit on \; \
        new-window -t "$SESSION:" -n frontend -c "$ROOT_DIR/frontend" \
        "$BASH" "$ROOT_DIR/scripts/dev.sh" _frontend \; \
        set-option -w -t "$SESSION:frontend" remain-on-exit on \; \
        select-window -t "$SESSION:frontend"
    compose config --format json | node "$ROOT_DIR/scripts/dev.mjs" wait "$FRONTEND_PORT"
    enter_session
}

stop() {
    # 先终止启动流程，防止停止期间构建完成后又创建后端容器。
    if tmux has-session -t "=$SESSION" 2> /dev/null; then
        tmux kill-session -t "=$SESSION"
    fi
    check_docker
    if docker container inspect "$BUILD_CONTAINER" > /dev/null 2>&1; then
        docker stop --time 1 "$BUILD_CONTAINER" > /dev/null
    fi
    if docker container inspect "$CONTAINER" > /dev/null 2>&1; then
        docker stop --time 45 "$CONTAINER" > /dev/null
    fi
    echo "开发环境已停止，临时容器自动删除；配置与构建缓存保留。"
}

case "${1:-start}" in
    start | stop | restart | build)
        require_command tmux
        require_command docker
        case "${1:-start}" in
            start) start ;;
            stop) stop ;;
            restart)
                stop
                start
                ;;
            build)
                check_docker
                if [[ "$(docker inspect --format '{{.State.Running}}' "$CONTAINER" 2> /dev/null || true)" != true ]]; then
                    echo "开发后端尚未运行，请先执行 ./scripts/dev.sh start。" >&2
                    exit 1
                fi
                build_backend
                reload_backend
                ;;
        esac
        ;;
    _backend)
        compose build openvpn-web
        build_backend
        compose run --detach --rm --no-deps --service-ports --name "$CONTAINER" openvpn-web
        exec docker logs --follow "$CONTAINER"
        ;;
    _frontend)
        cd frontend
        pnpm install --frozen-lockfile
        exec pnpm dev --port "$FRONTEND_PORT" --strictPort
        ;;
    *)
        echo "用法：$0 {start|stop|restart|build}" >&2
        exit 2
        ;;
esac
