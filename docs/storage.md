# 存储与权限

[返回项目首页](../README.md) · 本文命令默认在仓库根目录执行。

只需挂载 `./config:/config`：

```text
/config/config.yaml                         # 全部 profile 配置，0600
/config/secrets/<UUID>                       # 证书和私钥原文，root:root 0600
/config/logs/*.log                           # Supervisor 和各应用日志
/config/.state/                             # 上次 endpoint、账户 UID 等运行记录
/config/profiles/<profile-id>/ssh/home/      # 各配置独立 SSH home
/config/profiles/<profile-id>/ssh/ssh_host_* # 各配置独立 SSH 宿主密钥
```

`secrets`、`.state`
和日志目录为 root 所有、0700；YAML 中证书和私钥字段只保存 UUID，不存 PEM 原文。其他服务密码保存在 root 0600 的 YAML 中。SSH 用户使用独立 UID，无 sudo 时无法读取管理配置、私钥或其他 profile 的 home。管理员可在认证后的页面读取和编辑原文；宿主机 root 和授予 sudo 的 SSH 用户仍可访问这些内容。

运行配置与临时凭据在
`/run/vpnweb`，Compose 使用 tmpfs。日志持久化，Supervisor 自动轮转，每个主要服务最多约 15 MB。备份需要保留整个 `config` 目录及权限。旧密钥文件保留用于恢复，不会自动删除。

```bash
docker compose logs --tail=100
docker compose exec openvpn-web supervisorctl status
docker compose exec openvpn-web tail -n 100 /config/logs/bootstrap.log
docker compose exec openvpn-web tail -n 100 /config/logs/openvpn.log
```

## YAML 结构

首次启动自动生成完整默认值。建议通过界面编辑；需要手动改 YAML 时先停止容器，避免覆盖正在保存的内容。

```yaml
active_profile: <profile-uuid>
profiles:
  - id: <profile-uuid>
    name: 工作 VPN
    settings:
      vpn:
        config: |-
          client
          remote-cert-tls server
        selection_mode: auto
        selected_endpoint: endpoint-1
        endpoints:
          - id: endpoint-1
            name: 主节点
            host: vpn.example.com
            port: 1194
            protocol: udp
        ca: <secret-uuid>
        cert: <secret-uuid>
        key: <secret-uuid>
        dns_mode: manual
        dns: [1.1.1.1]
        httpdns_url: https://doh.pub/dns-query
        httpdns_ips: [1.12.12.12, 120.53.53.53]
        httpdns_protocol: doh
        autoconnect: true
      http:
        enabled: true
      socks5:
        enabled: false
      sshd:
        enabled: false
      health:
        enabled: false
        failure_threshold: 3
        action: restart
        action_cooldown_secs: 120
```

以上是结构示例，UUID 占位值必须替换为有效的 profile / secret ID。密钥文件需要存在于 `secrets`
中。其余字段由程序补默认值。

日常配置和服务操作见[配置与服务](configuration.md)；容器挂载与启动方式见[部署与启动](deployment.md)。
