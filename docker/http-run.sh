#!/bin/sh
set -eu
pipe=/run/tinyproxy/log.pipe
[ -p "$pipe" ] || mkfifo "$pipe"
chown tinyproxy:tinyproxy "$pipe"
chmod 600 "$pipe"
# 降权后的 Tinyproxy 不能重新打开 Supervisor 的 root 管道，用 FIFO 转交日志。
cat "$pipe" &
exec /usr/bin/tinyproxy -d -c /run/vpnweb/tinyproxy.conf
