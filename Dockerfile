# syntax=docker/dockerfile:1
FROM debian:13-slim AS socks-builder
# hadolint ignore=DL3008
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl build-essential libcrypt-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
RUN curl -fsSLO https://www.inet.no/dante/files/dante-1.4.4.tar.gz \
    && printf '%s\n' '1973c7732f1f9f0a4c0ccf2c1ce462c7c25060b25643ea90f9b98f53a813faec  dante-1.4.4.tar.gz' > dante-1.4.4.tar.gz.sha256 \
    && sha256sum -c dante-1.4.4.tar.gz.sha256 \
    && tar -xzf dante-1.4.4.tar.gz
WORKDIR /build/dante-1.4.4
RUN ./configure --prefix=/usr/local --disable-client --without-pam --without-libwrap --without-upnp \
    && make -j"$(nproc)" \
    && make install

FROM debian:13-slim AS runtime
LABEL org.opencontainers.image.title="OpenVPN Web" \
      org.opencontainers.image.description="OpenVPN with a Rust/React console, HTTP/SOCKS5 proxies, and optional SSH" \
      org.opencontainers.image.source="https://github.com/loveyu/openvpn-web-docker" \
      org.opencontainers.image.licenses="MIT"
ENV DEBIAN_FRONTEND=noninteractive \
    CONFIG_DIR=/config \
    TZ=Asia/Shanghai \
    PYTHONUNBUFFERED=1
# hadolint ignore=DL3008
RUN printf '#!/bin/sh\nexit 101\n' > /usr/sbin/policy-rc.d \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl tzdata supervisor python3 openvpn tinyproxy \
        openssh-server nftables iproute2 bind9-dnsutils sudo \
    && rm -rf /var/lib/apt/lists/* /etc/ssh/ssh_host_* \
    && mkdir -p /config /app/frontend /usr/local/lib/vpnweb
COPY --from=socks-builder /usr/local/sbin/sockd /usr/sbin/danted
# hadolint ignore=DL3008
RUN apt-get update && apt-get install -y --no-install-recommends iputils-ping procps mtr-tiny traceroute \
    && rm -rf /var/lib/apt/lists/*

FROM runtime AS development
ENV VPNWEB_DEV=1
COPY docker/http-run.sh /usr/local/lib/vpnweb/http-run.sh
COPY docker/supervisord.conf /etc/supervisor/supervisord.conf
COPY docker/supervisord.dev.conf /etc/supervisor/supervisord.dev.conf
COPY --chmod=755 docker/entrypoint.sh /usr/local/bin/vpnweb-entrypoint
RUN cat /etc/supervisor/supervisord.dev.conf >> /etc/supervisor/supervisord.conf \
    && ln -s /app/dev/openvpn-web /usr/local/bin/openvpn-web
ENTRYPOINT ["/usr/local/bin/vpnweb-entrypoint"]

FROM --platform=$BUILDPLATFORM node:26.13.1-trixie-slim AS frontend
WORKDIR /build/frontend
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN npm install --global --no-update-notifier --allow-scripts=pnpm pnpm@12.11.2
RUN --mount=type=cache,id=vpnweb-pnpm,target=/pnpm/store \
    pnpm install --frozen-lockfile --store-dir=/pnpm/store --loglevel error
COPY frontend/ ./
RUN pnpm build

# CI 用同名构建上下文传入一次构建的 dist；本地继续在镜像内构建前端。
FROM scratch AS frontend-assets
COPY --from=frontend /build/frontend/dist/ /

FROM rust:1.100.0-slim-trixie AS backend
ARG TARGETARCH
WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src/ src/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,id=vpnweb-target-${TARGETARCH},target=/build/target \
    cargo build --release --locked && cp target/release/openvpn-web /usr/local/bin/openvpn-web

FROM runtime
COPY --from=backend /usr/local/bin/openvpn-web /usr/local/bin/openvpn-web
COPY --from=frontend-assets / /app/frontend/
COPY docker/http-run.sh /usr/local/lib/vpnweb/http-run.sh
COPY docker/supervisord.conf /etc/supervisor/supervisord.conf
COPY --chmod=755 docker/entrypoint.sh /usr/local/bin/vpnweb-entrypoint
EXPOSE 80 8888 1080 22
VOLUME ["/config"]
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl --fail --silent --max-time 3 http://127.0.0.1/healthz
ENTRYPOINT ["/usr/local/bin/vpnweb-entrypoint"]
