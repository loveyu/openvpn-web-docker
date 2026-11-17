#!/bin/sh
set -eu
umask 077

if [ ! -c /dev/net/tun ]; then
    echo '缺少 /dev/net/tun，请按 compose.yaml 添加设备与 NET_ADMIN 权限。' >&2
    exit 1
fi
mkdir -p /run/vpnweb /run/sshd /run/tinyproxy "${CONFIG_DIR:-/config}/logs" "${CONFIG_DIR:-/config}"
mkdir -p "${CONFIG_DIR:-/config}/.state" "${CONFIG_DIR:-/config}/secrets"
# bind mount 可能原属宿主机 UID 1000；不能让同 UID 的 SSH 用户拥有配置目录。
chown root:root "${CONFIG_DIR:-/config}" "${CONFIG_DIR:-/config}/.state" "${CONFIG_DIR:-/config}/secrets" "${CONFIG_DIR:-/config}/logs"
chmod 700 "${CONFIG_DIR:-/config}/.state"
chmod 700 "${CONFIG_DIR:-/config}/secrets"
if [ -f "${CONFIG_DIR:-/config}/config.yaml" ]; then
    chown root:root "${CONFIG_DIR:-/config}/config.yaml"
    chmod 600 "${CONFIG_DIR:-/config}/config.yaml"
fi
chmod 711 /run/vpnweb
chmod 700 "${CONFIG_DIR:-/config}/logs"
chown tinyproxy:tinyproxy /run/tinyproxy
/usr/local/bin/openvpn-web --init
# 首个网络服务启动前完成默认拒绝策略，失败即退出容器。
/usr/local/bin/openvpn-web runtime init
exec /usr/bin/supervisord -n -c /etc/supervisor/supervisord.conf
