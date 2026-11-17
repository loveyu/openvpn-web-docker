# syntax=docker/dockerfile:1
FROM debian:13-slim AS socks-builder
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl build-essential libcrypt-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
RUN curl -fsSLO https://www.inet.no/dante/files/dante-1.4.4.tar.gz \
    && echo '1973c7732f1f9f0a4c0ccf2c1ce462c7c25060b25643ea90f9b98f53a813faec  dante-1.4.4.tar.gz' | sha256sum -c - \
    && tar -xzf dante-1.4.4.tar.gz \
    && cd dante-1.4.4 \
    && ./configure --prefix=/usr/local --disable-client --without-pam --without-libwrap --without-upnp \
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
RUN printf '#!/bin/sh\nexit 101\n' > /usr/sbin/policy-rc.d \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl tzdata supervisor python3 openvpn tinyproxy \
        openssh-server nftables iproute2 bind9-dnsutils sudo \
    && rm -rf /var/lib/apt/lists/* /etc/ssh/ssh_host_* \
    && mkdir -p /config /app/frontend /usr/local/lib/vpnweb
COPY --from=socks-builder /usr/local/sbin/sockd /usr/sbin/danted
RUN apt-get update && apt-get install -y --no-install-recommends iputils-ping procps \
    && rm -rf /var/lib/apt/lists/*

FROM node:26.13.1-trixie-slim AS frontend
WORKDIR /build/frontend
COPY frontend/package*.json ./
RUN --mount=type=cache,target=/root/.npm npm ci
COPY frontend/ ./
RUN npm run build

FROM rust:1.100.0-slim-trixie AS backend
WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src/ src/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo build --release --locked && cp target/release/openvpn-web /usr/local/bin/openvpn-web

FROM runtime
COPY --from=backend /usr/local/bin/openvpn-web /usr/local/bin/openvpn-web
COPY --from=frontend /build/frontend/dist/ /app/frontend/
COPY docker/http-run.sh /usr/local/lib/vpnweb/http-run.sh
COPY docker/supervisord.conf /etc/supervisor/supervisord.conf
COPY --chmod=755 docker/entrypoint.sh /usr/local/bin/vpnweb-entrypoint
EXPOSE 8080 8888 1080 2222
VOLUME ["/config"]
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD port="$(cat /run/vpnweb/web-port)" && curl --fail --silent --max-time 3 "http://127.0.0.1:$port/healthz"
ENTRYPOINT ["/usr/local/bin/vpnweb-entrypoint"]
