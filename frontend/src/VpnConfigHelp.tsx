import { useRef } from "react";
import { CircleHelp, X } from "lucide-react";

const commands = [
  { name: "client", example: "client", description: "启用客户端模式，接收服务器下发的连接参数。" },
  { name: "dev", example: "dev tun", description: "选择虚拟网卡类型；本容器统一使用 tun0。" },
  {
    name: "proto",
    example: "proto udp",
    description: "选择 UDP 或 TCP 传输；填写节点列表后以节点协议为准。",
  },
  {
    name: "remote",
    example: "remote vpn.example.com 1194",
    description: "指定服务器地址和端口；填写节点列表后由列表接管。",
  },
  { name: "port", example: "port 1194", description: "设置默认端口；remote 中指定的端口优先。" },
  { name: "rport", example: "rport 1194", description: "设置远端服务器的默认端口。" },
  { name: "nobind", example: "nobind", description: "不固定客户端本地端口，由系统分配。" },
  {
    name: "resolv-retry",
    example: "resolv-retry infinite",
    description: "设置域名解析失败后的重试时间；infinite 表示持续重试。",
  },
  { name: "persist-key", example: "persist-key", description: "软重连时保留已读取的密钥。" },
  { name: "persist-tun", example: "persist-tun", description: "软重连时保留隧道网卡。" },
  {
    name: "remote-cert-tls",
    example: "remote-cert-tls server",
    description: "验证对端证书具有服务器用途。",
  },
  {
    name: "verify-x509-name",
    example: "verify-x509-name vpn.example.com name",
    description: "校验服务器证书的名称。",
  },
  {
    name: "auth-user-pass",
    example: "auth-user-pass",
    description: "启用用户名密码认证；凭据需填写在页面表单中。",
  },
  {
    name: "auth-nocache",
    example: "auth-nocache",
    description: "使用后清除内存中的认证密码；容器自动添加此选项。",
  },
  {
    name: "ca",
    example: "ca ca.crt",
    description: "指定信任的 CA；外部文件内容需填入证书表单，也可使用 <ca> 内联。",
  },
  {
    name: "cert",
    example: "cert client.crt",
    description: "指定客户端证书，支持证书表单或 <cert> 内联。",
  },
  {
    name: "key",
    example: "key client.key",
    description: "指定客户端私钥，支持密钥表单或 <key> 内联。",
  },
  {
    name: "tls-auth",
    example: "tls-auth ta.key 1",
    description: "使用静态密钥验证控制通道；外部密钥及方向由页面表单管理。",
  },
  {
    name: "key-direction",
    example: "key-direction 1",
    description: "设置 tls-auth 密钥方向；通常客户端为 1、服务器为 0。",
  },
  {
    name: "tls-crypt",
    example: "tls-crypt tls-crypt.key",
    description: "加密并验证控制通道；支持密钥表单或内联。",
  },
  {
    name: "data-ciphers",
    example: "data-ciphers AES-256-GCM:AES-128-GCM",
    description: "设置可协商的数据通道加密算法。",
  },
  {
    name: "cipher",
    example: "cipher AES-256-CBC",
    description: "设置旧版数据通道加密算法；新版本主要使用 data-ciphers。",
  },
  {
    name: "auth",
    example: "auth SHA256",
    description: "设置数据包认证摘要算法；AEAD 算法通常自带认证。",
  },
  {
    name: "comp-lzo",
    example: "comp-lzo no",
    description: "兼容旧版 LZO 压缩设置，需要与服务器匹配。",
  },
  {
    name: "compress",
    example: "compress stub-v2",
    description: "设置压缩或兼容模式，需要与服务器匹配。",
  },
  {
    name: "pull",
    example: "pull",
    description: "接收服务器下发的路由等选项；client 已包含此行为。",
  },
  {
    name: "route",
    example: "route 10.0.0.0 255.0.0.0",
    description: "添加到指定网段的静态路由；仅“按配置”模式保留。",
    routing: true,
  },
  {
    name: "route-ipv6",
    example: "route-ipv6 2001:db8::/32",
    description: "声明 IPv6 路由；本容器的隧道外 IPv6 始终受出口保护。",
    routing: true,
  },
  {
    name: "route-nopull",
    example: "route-nopull",
    description: "不安装服务器下发的路由；DNS 仍由页面单独管理。",
    routing: true,
  },
  {
    name: "redirect-gateway",
    example: "redirect-gateway def1",
    description: "将默认流量重定向到 VPN；“全部”模式统一使用 def1。",
    routing: true,
  },
  {
    name: "redirect-private",
    example: "redirect-private",
    description: "调整到 VPN 端点的路由，同时保留原默认路由。",
    routing: true,
  },
  {
    name: "route-gateway",
    example: "route-gateway 10.8.0.1",
    description: "设置 VPN 路由使用的默认网关。",
    routing: true,
  },
  {
    name: "route-metric",
    example: "route-metric 800",
    description: "设置 VPN 路由的默认优先级数值；通常越小越优先。",
    routing: true,
  },
  {
    name: "pull-filter",
    example: 'pull-filter ignore "route "',
    description: "按前缀接收、忽略或拒绝服务器下发的选项；“全部”模式忽略限制路由的过滤器。",
  },
  {
    name: "dhcp-option",
    example: "dhcp-option DNS 10.8.0.1",
    description: "声明 DNS 等参数；实际 DNS 由页面的 DNS 模式决定。",
  },
  { name: "ping", example: "ping 10", description: "空闲时每隔指定秒数发送保活包。" },
  {
    name: "ping-restart",
    example: "ping-restart 60",
    description: "在指定秒数内未收到数据时软重连。",
  },
  { name: "keepalive", example: "keepalive 10 60", description: "同时设置保活间隔和超时重连。" },
  {
    name: "connect-timeout",
    example: "connect-timeout 10",
    description: "设置连接服务器时的超时秒数。",
  },
  {
    name: "verb",
    example: "verb 3",
    description: "设置日志详细程度；容器运行配置统一追加级别 3。",
  },
] as const;

function configuredCommands(config: string) {
  const used = new Set<string>();
  let inline = "";
  for (const line of config.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (inline) {
      if (trimmed === `</${inline}>`) inline = "";
      continue;
    }
    const tag = trimmed.match(/^<([a-z-]+)>$/i)?.[1];
    if (tag) {
      used.add(tag);
      inline = tag;
      continue;
    }
    const name = trimmed.match(/^(?:--)?([a-z][a-z0-9-]*)(?:\s|$)/i)?.[1];
    if (name) used.add(name);
  }
  return used;
}

export function VpnConfigHelp({
  config,
  routeMode,
}: {
  config: string;
  routeMode: "all" | "config";
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const used = configuredCommands(config);
  const sorted = commands.toSorted((a, b) => Number(used.has(b.name)) - Number(used.has(a.name)));

  return (
    <>
      <button
        type="button"
        className="button small config-help-button"
        onClick={() => dialog.current?.showModal()}
      >
        <CircleHelp size={16} />
        配置命令帮助
      </button>
      <dialog
        ref={dialog}
        className="modal config-help-dialog"
        aria-labelledby="config-help-title"
        onClick={(event) => {
          if (event.target === event.currentTarget) dialog.current?.close();
        }}
      >
        <div className="config-help-head">
          <h2 id="config-help-title" tabIndex={-1} autoFocus>
            OpenVPN 配置命令
          </h2>
          <button
            type="button"
            className="icon-button"
            aria-label="关闭配置命令帮助"
            onClick={() => dialog.current?.close()}
          >
            <X size={20} />
          </button>
        </div>
        <p>当前原文中的指令已高亮并置顶，注释不计入。最终生效参数由路由模式和容器管理规则决定。</p>
        <div className="config-help-list">
          {sorted.map((command) => {
            const active = used.has(command.name);
            const ignored = active && routeMode === "all" && "routing" in command;
            return (
              <article
                key={command.name}
                className={`config-command ${active ? "configured" : ""}`}
              >
                <div>
                  <code>{command.name}</code>
                  {active && <span>{ignored ? "全部模式忽略原文" : "原文已配置"}</span>}
                </div>
                <p>{command.description}</p>
                <code className="command-example">{command.example}</code>
              </article>
            );
          })}
        </div>
      </dialog>
    </>
  );
}
