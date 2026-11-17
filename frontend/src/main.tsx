import { StrictMode, useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import {
  HashRouter,
  NavLink,
  Navigate,
  Route,
  Routes,
  useNavigate,
  useParams,
  useSearchParams,
} from "react-router-dom";
import { request } from "./api";
import { LogOutput } from "./LogOutput";
import { NetworkProbes } from "./NetworkProbes";
import { TraceProbe } from "./TraceProbe";
import { VpnConfigHelp } from "./VpnConfigHelp";
import { useToolPreference } from "./useToolPreference";
import {
  Activity,
  ArrowDownToLine,
  ArrowRight,
  Check,
  ChevronRight,
  CircleHelp,
  FileKey2,
  Globe2,
  KeyRound,
  LayoutDashboard,
  LoaderCircle,
  LockKeyhole,
  Monitor,
  Moon,
  Network,
  Play,
  Plus,
  GripVertical,
  Trash2,
  RefreshCw,
  Save,
  Settings2,
  ShieldCheck,
  Square,
  Sun,
  Terminal,
  Upload,
  X,
} from "lucide-react";
import type {
  Config,
  ConfigDocument,
  DnsResult,
  DnsResults,
  ProxyConfig,
  Service,
  Status,
  VpnHealth,
} from "./types";
import "./style.css";

type Tab = "overview" | "vpn" | "http" | "socks5" | "sshd" | "logs" | "health" | "tools";
const nav = [
  {
    id: "overview",
    label: "运行概览",
    icon: LayoutDashboard,
    group: "工作空间",
  },
  { id: "vpn", label: "OpenVPN", icon: ShieldCheck, group: "连接与服务" },
  { id: "http", label: "HTTP 代理", icon: Globe2 },
  { id: "socks5", label: "SOCKS5 代理", icon: Network },
  { id: "sshd", label: "SSH 访问", icon: Terminal },
  { id: "health", label: "健康检查", icon: ShieldCheck, group: "系统" },
  { id: "tools", label: "网络工具", icon: Network },
  { id: "logs", label: "服务日志", icon: Activity },
] as const;
const names: Record<Service, string> = {
  openvpn: "OpenVPN",
  http: "HTTP 代理",
  socks5: "SOCKS5 代理",
  sshd: "SSH 访问",
  backend: "管理服务",
};
const descriptions: Record<Tab, string> = {
  overview: "查看 VPN 连接与服务状态。",
  vpn: "管理节点、连接配置与证书。",
  http: "为浏览器与应用提供经过 VPN 隧道的 HTTP / HTTPS 代理。",
  socks5: "按需开启 SOCKS5 TCP 代理，支持无认证或用户名密码认证。",
  sshd: "通过 SSH 建立加密连接和端口转发，可选择允许终端登录。",
  logs: "查看各服务的近期输出，定位连接与配置问题。",
  health: "分别查看容器与 OpenVPN 健康状态，配置隧道探测与自动恢复。",
  tools: "选择网络工具，检查连通性、HTTP 响应、路由路径与 DNS 解析。",
};

const vpnHealthLabels: Record<VpnHealth["state"], string> = {
  unconfigured: "未配置",
  stopped: "已停止",
  connecting: "连接中",
  checking: "检测中",
  healthy: "正常",
  unhealthy: "异常",
};
const healthActionLabels: Record<Config["health"]["action"], string> = {
  none: "仅记录",
  reconnect: "软重连",
  restart: "重启 OpenVPN 服务",
  reapply: "重新应用 VPN 配置并重连",
};

function Field({
  label,
  help,
  children,
  wide = false,
}: {
  label: string;
  help?: string;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <label className={`field ${wide ? "wide" : ""}`}>
      <span>{label}</span>
      {children}
      {help && <small>{help}</small>}
    </label>
  );
}
function Toggle({
  checked,
  onChange,
  label,
  help,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  help?: string;
}) {
  return (
    <div className="toggle-row">
      <div>
        <strong>{label}</strong>
        {help && <p>{help}</p>}
      </div>
      <button
        type="button"
        role="switch"
        aria-label={label}
        aria-checked={checked}
        className={`toggle ${checked ? "on" : ""}`}
        onClick={() => onChange(!checked)}
      >
        <span />
      </button>
    </div>
  );
}
function Panel({
  title,
  description,
  children,
  aside,
}: {
  title: string;
  description?: string;
  children: ReactNode;
  aside?: ReactNode;
}) {
  return (
    <section className="panel">
      <div className="panel-head">
        <div>
          <h2>{title}</h2>
          {description && <p>{description}</p>}
        </div>
        {aside}
      </div>
      {children}
    </section>
  );
}
function AuthFields({
  value,
  onChange,
}: {
  value: ProxyConfig;
  onChange: (value: ProxyConfig) => void;
}) {
  const auth = value.authentication === "password";
  return (
    <>
      <Field label="认证方式" wide>
        <select
          value={auth ? "password" : "none"}
          onChange={(event) => {
            const authentication = event.target.value as "none" | "password";
            onChange({
              ...value,
              authentication,
              ...(authentication === "none" ? { username: "", password: "" } : {}),
            });
          }}
        >
          <option value="none">无认证</option>
          <option value="password">用户名和密码</option>
        </select>
      </Field>
      {auth && (
        <>
          <Field label="用户名">
            <input
              autoComplete="off"
              value={value.username}
              onChange={(event) => onChange({ ...value, username: event.target.value })}
              placeholder="proxyuser"
            />
          </Field>
          <Field label="密码">
            <input
              type="password"
              autoComplete="new-password"
              value={value.password}
              onChange={(event) => onChange({ ...value, password: event.target.value })}
            />
          </Field>
        </>
      )}
    </>
  );
}
function Badge({ state }: { state?: string }) {
  const label =
    state === "RUNNING"
      ? "运行中"
      : state === "STARTING"
        ? "启动中"
        : state === "FATAL" || state === "BACKOFF"
          ? "异常"
          : "已停止";
  return (
    <span
      className={`badge ${state === "RUNNING" ? "green" : state === "FATAL" || state === "BACKOFF" ? "red" : ""}`}
    >
      <i />
      {label}
    </span>
  );
}

function DnsTable({
  rows,
  emptyMessage = "探测中，后台异步更新结果…",
}: {
  rows: DnsResult[];
  emptyMessage?: string;
}) {
  const grouped = new Map<string, { ipv4?: DnsResult; ipv6?: DnsResult }>();
  for (const row of rows) {
    if (row.record_type === "AAAA" && !row.ok) continue;
    const key = `${row.provider_id ?? row.provider}-${row.ip}`;
    const group = grouped.get(key) ?? {};
    group[row.record_type === "AAAA" ? "ipv6" : "ipv4"] = row;
    grouped.set(key, group);
  }
  return (
    <div className="table-scroll dns-table-scroll">
      <table className="results-table dns-results-table">
        <thead>
          <tr>
            <th>服务</th>
            <th>服务器 IP</th>
            <th>状态</th>
            <th>解析结果</th>
          </tr>
        </thead>
        <tbody>
          {[...grouped.entries()].map(([key, { ipv4, ipv6 }]) => {
            const row = (ipv4?.ok ? ipv4 : ipv6) ?? ipv4!;
            return (
              <tr key={key}>
                <td className="dns-provider">{row.provider}</td>
                <td className="dns-server" data-label="服务器">
                  <code>{row.ip}</code>
                </td>
                <td className="dns-timing">
                  <span className={`dns-outcome ${row.ok ? "success" : "error"}`}>
                    {row.ok ? "成功" : "失败"}
                  </span>
                </td>
                <td className={`dns-answer ${row.ok ? "" : "dns-error"}`}>
                  {[ipv4, ipv6]
                    .filter((result): result is DnsResult => result?.ok === true)
                    .map((result) => (
                      <div className="dns-family" key={result.record_type ?? "A"}>
                        <span className="dns-record-type">
                          {result.record_type === "AAAA" ? "IPv6" : "IPv4"}
                          <span>{result.latency_ms.toFixed(1)} ms</span>
                        </span>
                        <div className="dns-addresses">
                          {result.answers.map((answer) => (
                            <code key={answer}>{answer}</code>
                          ))}
                        </div>
                      </div>
                    ))}
                  {!row.ok && <code>{row.error}</code>}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {rows.length === 0 && <p className="table-empty">{emptyMessage}</p>}
    </div>
  );
}

type Theme = "system" | "light" | "dark";

function ThemeSwitcher() {
  const [theme, setTheme] = useState<Theme>(
    () => (document.documentElement.dataset.theme as Theme) || "system",
  );
  const options = [
    { value: "system", label: "跟随系统", icon: Monitor },
    { value: "light", label: "亮色模式", icon: Sun },
    { value: "dark", label: "暗色模式", icon: Moon },
  ] as const;
  const currentIndex = options.findIndex((option) => option.value === theme);
  const current = options[currentIndex];
  const next = options[(currentIndex + 1) % options.length];
  function changeTheme(value: Theme) {
    setTheme(value);
    document.documentElement.dataset.theme = value;
    try {
      localStorage.setItem("vpnweb-theme", value);
    } catch {
      // 本地存储不可用时，当前页面的主题选择仍然生效。
    }
  }

  return (
    <div className="theme-switcher" role="group" aria-label="外观模式">
      {options.map(({ value, label, icon: Icon }) => (
        <button
          key={value}
          type="button"
          className="theme-option"
          title={label}
          aria-label={label}
          aria-pressed={theme === value}
          onClick={() => changeTheme(value)}
        >
          <Icon size={18} />
        </button>
      ))}
      <button
        type="button"
        className="theme-cycle"
        title={`${current.label}，切换为${next.label}`}
        aria-label={`${current.label}，切换为${next.label}`}
        onClick={() => changeTheme(next.value)}
      >
        <current.icon size={18} />
      </button>
    </div>
  );
}

function App({ tab }: { tab: Tab }) {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { tool: requestedTool = "" } = useParams();
  const navigation = useRef<HTMLElement>(null);
  useEffect(() => {
    window.scrollTo(0, 0);
    if (window.matchMedia("(max-width: 760px)").matches) {
      navigation.current?.querySelector(".nav-item.selected")?.scrollIntoView({
        block: "nearest",
        inline: "center",
      });
    }
  }, [tab, requestedTool]);
  const selectedTool = ["ping", "tcping", "curl", "dns", "mtr", "traceroute"].includes(
    requestedTool,
  )
    ? requestedTool
    : "";
  const setTab = (value: Tab) => navigate(`/${value}`);
  const [documentConfig, setDocumentConfig] = useState<ConfigDocument>();
  const profile = documentConfig?.profiles.find(
    (item) => item.id === documentConfig.active_profile,
  );
  const config = profile?.settings;
  const automaticEndpoint = config?.vpn.selection_mode === "auto";
  const automaticDns = config?.vpn.dns_mode === "pushed";
  const [newProfile, setNewProfile] = useState(false);
  const authHelp = useRef<HTMLDialogElement>(null);
  // 部署配置不是登录表单，仅在获焦时开放编辑，避免浏览器回填管理凭据。
  const [sshCredentialFocus, setSshCredentialFocus] = useState<"username" | "password">();
  const [newProfileName, setNewProfileName] = useState("");
  const [dnsDomain, setDnsDomain] = useToolPreference<string>("dns-domain", "example.com");
  const [dnsResults, setDnsResults] = useState<DnsResults>();
  const [status, setStatus] = useState<Status>();
  const [healthTest, setHealthTest] = useState<VpnHealth>();
  useEffect(() => setHealthTest(undefined), [documentConfig?.active_profile]);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<{ text: string; error?: boolean }>();
  const requestedLogService = searchParams.get("service") ?? "openvpn";
  const logService: Service = Object.hasOwn(names, requestedLogService)
    ? (requestedLogService as Service)
    : "openvpn";
  const [log, setLog] = useState("");
  const upload = useRef<HTMLInputElement>(null);

  const notify = useCallback((text: string, error = false) => setNotice({ text, error }), []);
  const refresh = useCallback(async () => setStatus(await request<Status>("/status")), []);
  useEffect(() => {
    let cancelled = false;
    Promise.all([request<ConfigDocument>("/config"), request<Status>("/status")])
      .then(([settings, state]) => {
        if (!cancelled) {
          setDocumentConfig(settings);
          setStatus(state);
          setDirty(false);
        }
      })
      .catch((error) => {
        if (!cancelled) notify(error.message, true);
      });
    const timer = setInterval(() => {
      if (!document.hidden) refresh().catch((error) => notify(error.message, true));
    }, 5000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [refresh, notify]);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(undefined), 6500);
    return () => clearTimeout(timer);
  }, [notice]);
  useEffect(() => {
    const beforeUnload = (event: BeforeUnloadEvent) => {
      if (dirty) event.preventDefault();
    };
    window.addEventListener("beforeunload", beforeUnload);
    return () => window.removeEventListener("beforeunload", beforeUnload);
  }, [dirty]);
  const loadLogs = useCallback(async () => {
    const response = await request<{ content: string }>(`/logs/${logService}?lines=300`);
    setLog(response.content);
  }, [logService]);
  useEffect(() => {
    if (tab === "logs") loadLogs().catch((error) => notify(error.message, true));
  }, [tab, loadLogs, notify]);

  function update<K extends keyof Config>(section: K, value: Config[K]) {
    setDocumentConfig((current) => {
      if (!current) return current;
      return {
        ...current,
        profiles: current.profiles.map((item) =>
          item.id === current.active_profile
            ? { ...item, settings: { ...item.settings, [section]: value } }
            : item,
        ),
      };
    });
    setDirty(true);
  }
  function moveEndpoint(id: string, target: number) {
    if (!config) return;
    const endpoints = [...config.vpn.endpoints];
    const source = endpoints.findIndex((item) => item.id === id);
    if (source < 0 || target < 0 || target >= endpoints.length) return;
    endpoints.splice(target, 0, endpoints.splice(source, 1)[0]);
    update("vpn", { ...config.vpn, endpoints });
  }
  async function execute(work: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    try {
      await work();
    } catch (error) {
      notify(error instanceof Error ? error.message : "操作失败", true);
    } finally {
      setBusy(false);
    }
  }
  async function save() {
    const result = await request<{ config: ConfigDocument }>("/config", {
      method: "PUT",
      body: JSON.stringify(documentConfig),
    });
    setDocumentConfig(result.config);
    setDirty(false);
  }
  function control(service: Service, action: "start" | "stop" | "restart", saveFirst = false) {
    void execute(async () => {
      if (saveFirst || (service === "openvpn" && action !== "stop")) await save();
      const result = await request<{ message: string }>(`/services/${service}/${action}`, {
        method: "POST",
      });
      notify(result.message);
      if (service !== "backend") await refresh();
    });
  }

  const connected = status?.vpn.state === "CONNECTED";
  const serviceState = (name: Service) =>
    status?.services.find((item) => item.name === name)?.state;
  const currentNav = nav.find((item) => item.id === tab)!;
  const running =
    status?.services.filter(
      (item) =>
        ["openvpn", "http", "socks5", "sshd", "backend"].includes(item.name) &&
        item.state === "RUNNING",
    ).length ?? 0;
  const footer = (service: Service) => (
    <div className="form-footer">
      <span>
        <CircleHelp size={15} />
        重启后生效
      </span>
      <button
        className="button primary"
        disabled={busy}
        onClick={() => control(service, "restart", true)}
      >
        <RefreshCw size={15} />
        保存并重启
      </button>
    </div>
  );

  return (
    <div className={`app-shell ${tab}-page`}>
      <aside className="sidebar">
        <NavLink className="brand" to="/overview">
          <div className="brand-icon">
            <ShieldCheck size={25} />
          </div>
          <div>
            <strong>
              OpenVPN<span>WEB CONSOLE</span>
            </strong>
          </div>
        </NavLink>
        <nav aria-label="主导航" ref={navigation}>
          {nav.map((item) => (
            <div key={item.id}>
              {"group" in item && <div className="nav-group">{item.group}</div>}
              <NavLink
                to={`/${item.id}`}
                className={`nav-item ${tab === item.id ? "selected" : ""}`}
                aria-label={item.label}
                title={item.label}
              >
                <item.icon size={18} />
                <span className="nav-label">{item.label}</span>
                {tab === item.id && <ChevronRight size={15} />}
              </NavLink>
            </div>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <div className="environment">
            <span className="environment-dot" />
            <div>
              <strong>自托管实例</strong>
              <span>Debian 13 · Supervisor</span>
            </div>
            <span className="version">v{status?.version ?? "0.1.0"}</span>
          </div>
          <button
            type="button"
            className="auth-note"
            onClick={() => authHelp.current?.showModal()}
            aria-label="Basic Auth 设置说明"
          >
            <LockKeyhole size={13} />
            Basic Auth · {status?.basic_auth_enabled ? "已启用" : "未设置"}
          </button>
        </div>
      </aside>
      <div className="workspace">
        <header className="topbar">
          <div className="breadcrumb">
            工作空间
            <ChevronRight size={14} />
            <span>{currentNav.label}</span>
          </div>
          <div className="topbar-right">
            <button
              type="button"
              className="button auth-help-mobile"
              onClick={() => authHelp.current?.showModal()}
              aria-label="Basic Auth 设置说明"
              title="Basic Auth 设置说明"
            >
              <LockKeyhole size={18} />
            </button>
            <span className="live-dot" />
            <select
              aria-label="切换 VPN 配置"
              className="profile-select"
              disabled={busy || !documentConfig}
              value={documentConfig?.active_profile ?? ""}
              onChange={(event) => {
                const id = event.target.value;
                void execute(async () => {
                  if (dirty) await save();
                  try {
                    const result = await request<{ config: ConfigDocument }>(
                      `/profiles/${id}/activate`,
                      { method: "POST" },
                    );
                    setDocumentConfig(result.config);
                    setDirty(false);
                  } catch (error) {
                    setDocumentConfig(await request<ConfigDocument>("/config"));
                    setDirty(false);
                    throw error;
                  } finally {
                    await refresh();
                  }
                  notify("已切换 VPN 配置");
                });
              }}
            >
              {documentConfig?.profiles.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </select>
            <button
              className="button small new-profile-button"
              aria-label="新增配置"
              title="新增配置"
              disabled={busy}
              onClick={() => setNewProfile(true)}
            >
              <Plus size={14} />
              <span>新增配置</span>
            </button>
          </div>
          <ThemeSwitcher />
        </header>
        <main>
          <div className="page-heading">
            <div>
              <span className="eyebrow">NETWORK / {tab.toUpperCase()}</span>
              <h1>{currentNav.label}</h1>
              <p>{descriptions[tab]}</p>
            </div>
            {tab !== "tools" && tab !== "logs" && (tab !== "overview" || dirty) && (
              <div className="heading-actions">
                {dirty && <span className="unsaved">未保存的更改</span>}
                <button
                  className="button"
                  disabled={busy || !config}
                  onClick={() =>
                    void execute(async () => {
                      await save();
                      notify("配置已保存");
                    })
                  }
                >
                  <Save size={15} />
                  保存配置
                </button>
              </div>
            )}
          </div>
          {!config ? (
            <div className="empty-state">
              <LoaderCircle className="spin" />
              正在读取配置…
              <button className="button" onClick={() => window.location.reload()}>
                重试
              </button>
            </div>
          ) : (
            <>
              {tab === "overview" && (
                <>
                  <section className={`connection-hero ${connected ? "connected" : ""}`}>
                    <div className="hero-main">
                      <div className="hero-shield">
                        <ShieldCheck size={32} />
                      </div>
                      <div>
                        <h2>{connected ? "VPN 已连接" : "VPN 未连接"}</h2>
                        <p>
                          {connected
                            ? `隧道 IP ${status?.vpn.ip}`
                            : config.vpn.config.trim() || config.vpn.endpoints.length > 0
                              ? "连接后启用代理出口"
                              : "添加节点或导入 OpenVPN 配置"}
                        </p>
                      </div>
                    </div>
                    <button
                      className="button primary hero-button"
                      disabled={busy}
                      onClick={() =>
                        connected
                          ? control("openvpn", "stop")
                          : config.vpn.config.trim() || config.vpn.endpoints.length > 0
                            ? control("openvpn", "start")
                            : setTab("vpn")
                      }
                    >
                      {connected ? <Square size={15} /> : <ArrowRight size={16} />}
                      {connected
                        ? "断开连接"
                        : config.vpn.config.trim() || config.vpn.endpoints.length > 0
                          ? "建立连接"
                          : "配置连接"}
                    </button>
                    <div className="hero-grid" />
                  </section>
                  <div className="stats-grid">
                    <div className="stat">
                      <div className="stat-label">
                        <Activity size={16} />
                        运行服务
                      </div>
                      <strong>
                        {running}
                        <span>/ 5</span>
                      </strong>
                    </div>
                    <div className="stat">
                      <div className="stat-label">
                        <ShieldCheck size={16} />
                        出口保护
                      </div>
                      <strong className="mint">{status?.kill_switch ? "已开启" : "读取中"}</strong>
                      <p>断线时阻止直连</p>
                    </div>
                    <div className="stat">
                      <div className="stat-label">
                        <Network size={16} />
                        隧道 DNS
                      </div>
                      <strong className="mono dns-stat">
                        {status?.dns.effective.join(", ") || "未配置"}
                      </strong>
                      {status?.dns.source !== "none" && (
                        <p>{status?.dns.source === "manual" ? "手动指定" : "服务器下发"}</p>
                      )}
                    </div>
                  </div>
                  <Panel
                    title="服务状态"
                    aside={
                      <button
                        className="icon-button"
                        aria-label="刷新状态"
                        disabled={busy}
                        onClick={() => void execute(refresh)}
                      >
                        <RefreshCw size={16} />
                      </button>
                    }
                  >
                    <div className="service-list">
                      {(["openvpn", "http", "socks5", "sshd", "backend"] as Service[]).map(
                        (name, index) => {
                          const Icon = [ShieldCheck, Globe2, Network, Terminal, Settings2][index];
                          const service = status?.services.find((item) => item.name === name);
                          const uptime =
                            service?.state === "RUNNING"
                              ? service.description
                                  .match(/\buptime (.+)$/)?.[1]
                                  .replace(/(\d+) days?, /, "$1 天 ")
                              : undefined;
                          const port =
                            name === "openvpn"
                              ? "tun0"
                              : `:${status?.ports.find((item) => item.name === name)?.port ?? "—"}`;
                          return (
                            <div className="service-row" key={name}>
                              <div className="service-icon">
                                <Icon size={19} />
                              </div>
                              <div className="service-name">
                                <strong>{names[name]}</strong>
                              </div>
                              <code>{port}</code>
                              <div className="service-status">
                                <Badge state={serviceState(name)} />
                                {uptime && <span className="service-uptime">已运行 {uptime}</span>}
                              </div>
                              <div className="service-actions">
                                {name !== "backend" && (
                                  <button
                                    className="icon-button"
                                    disabled={busy}
                                    title={
                                      serviceState(name) === "RUNNING"
                                        ? `停止${names[name]}`
                                        : `启动${names[name]}`
                                    }
                                    aria-label={
                                      serviceState(name) === "RUNNING"
                                        ? `停止${names[name]}`
                                        : `启动${names[name]}`
                                    }
                                    onClick={() =>
                                      control(
                                        name,
                                        serviceState(name) === "RUNNING" ? "stop" : "start",
                                      )
                                    }
                                  >
                                    {serviceState(name) === "RUNNING" ? (
                                      <Square size={14} />
                                    ) : (
                                      <Play size={14} />
                                    )}
                                  </button>
                                )}
                                <button
                                  className="icon-button service-restart"
                                  disabled={busy}
                                  title={`重启${names[name]}`}
                                  aria-label={`重启${names[name]}`}
                                  onClick={() => control(name, "restart")}
                                >
                                  <RefreshCw size={15} />
                                </button>
                                {name !== "backend" && (
                                  <button
                                    className="icon-button"
                                    title={`配置${names[name]}`}
                                    aria-label={`配置${names[name]}`}
                                    onClick={() => setTab(name === "openvpn" ? "vpn" : name)}
                                  >
                                    <Settings2 size={15} />
                                  </button>
                                )}
                              </div>
                            </div>
                          );
                        },
                      )}
                    </div>
                  </Panel>
                  <Panel title="监听端口">
                    <div className="port-grid">
                      {status?.ports.map((item) => {
                        return (
                          <div className="port-card" key={item.name}>
                            <span>{names[item.name]}</span>
                            <span className="port-label">容器端口</span>
                            <strong className="mono">:{item.port}</strong>
                            <small className={item.listening ? "mint" : "muted"}>
                              {item.listening ? "正在监听" : "未监听"}
                            </small>
                            <div className="port-mapping">
                              {item.published_port ? (
                                <>
                                  <span>宿主机 → 容器</span>
                                  <code>:{item.published_port}</code>
                                  <ArrowRight size={14} aria-hidden="true" />
                                  <code>:{item.target_port}</code>
                                </>
                              ) : (
                                <span>宿主机映射未提供</span>
                              )}
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  </Panel>
                  <Panel title="IPv4 路由">
                    <div className="table-scroll">
                      <table className="results-table">
                        <thead>
                          <tr>
                            <th>目标网段</th>
                            <th>网关</th>
                            <th>接口</th>
                            <th>源地址</th>
                            <th>优先级</th>
                          </tr>
                        </thead>
                        <tbody>
                          {status?.routes?.map((route, index) => (
                            <tr key={index}>
                              <td>
                                <code>{route.dst === "default" ? "0.0.0.0/0" : route.dst}</code>
                              </td>
                              <td>
                                <code>{route.gateway ?? "直连"}</code>
                              </td>
                              <td>
                                <code>{route.dev ?? "—"}</code>
                              </td>
                              <td>
                                <code>{route.prefsrc ?? "—"}</code>
                              </td>
                              <td>{route.metric ?? "—"}</td>
                            </tr>
                          ))}
                          {!status?.routes?.length && (
                            <tr>
                              <td colSpan={5}>暂无 IPv4 路由</td>
                            </tr>
                          )}
                        </tbody>
                      </table>
                    </div>
                  </Panel>
                  <div className="note overview-health">
                    <ShieldCheck size={17} />
                    <p>
                      容器：{status?.health.container.healthy ? "正常" : "待就绪 / 异常"} · VPN：
                      {status ? vpnHealthLabels[status.health.vpn.state] : "读取中"}
                      {status?.health.container.errors.join("；")}
                    </p>
                    <button className="text-button" onClick={() => setTab("health")}>
                      健康检查 <ChevronRight size={14} />
                    </button>
                  </div>
                  <div className="note">
                    <LockKeyhole size={17} />
                    <p>隧道关闭后，管理页面与 SSH 入口仍可访问。</p>
                  </div>
                </>
              )}
              {tab === "vpn" && (
                <>
                  <div className="note">
                    <Activity size={17} />
                    <p>
                      VPN 健康：{status ? vpnHealthLabels[status.health.vpn.state] : "读取中"}
                      {status?.health.vpn.errors.join("；")}
                    </p>
                  </div>
                  <Panel
                    title="VPN 节点"
                    aside={
                      <button
                        className="button small"
                        onClick={() => {
                          const id =
                            globalThis.crypto?.randomUUID?.() ??
                            `endpoint-${Date.now()}-${Math.random().toString(16).slice(2)}`;
                          let number = 1;
                          while (
                            config.vpn.endpoints.some((item) => item.name === `Endpoint ${number}`)
                          ) {
                            number++;
                          }
                          update("vpn", {
                            ...config.vpn,
                            selected_endpoint: config.vpn.selected_endpoint || id,
                            endpoints: [
                              ...config.vpn.endpoints,
                              {
                                id,
                                name: `Endpoint ${number}`,
                                host: "",
                                port: 1194,
                                protocol: "tcp",
                              },
                            ],
                          });
                        }}
                      >
                        <Plus size={14} />
                        添加节点
                      </button>
                    }
                  >
                    <div className="panel-body">
                      <div className="form-grid compact-grid">
                        <Field label="配置名称">
                          <input
                            value={profile?.name ?? ""}
                            onChange={(event) => {
                              const name = event.target.value;
                              setDocumentConfig((current) =>
                                current
                                  ? {
                                      ...current,
                                      profiles: current.profiles.map((item) =>
                                        item.id === current.active_profile
                                          ? { ...item, name }
                                          : item,
                                      ),
                                    }
                                  : current,
                              );
                              setDirty(true);
                            }}
                          />
                        </Field>
                        <Field label="连接模式">
                          <select
                            value={config.vpn.selection_mode}
                            onChange={(event) =>
                              update("vpn", {
                                ...config.vpn,
                                selection_mode: event.target.value as "auto" | "manual",
                              })
                            }
                          >
                            <option value="auto">自动选择</option>
                            <option value="manual">手动指定</option>
                          </select>
                        </Field>
                        {!automaticEndpoint && (
                          <Field label="连接节点" wide>
                            <select
                              value={config.vpn.selected_endpoint}
                              disabled={config.vpn.endpoints.length === 0}
                              onChange={(event) =>
                                update("vpn", {
                                  ...config.vpn,
                                  selected_endpoint: event.target.value,
                                })
                              }
                            >
                              {config.vpn.endpoints.length === 0 && (
                                <option value="">使用 .ovpn 原文中的 remote</option>
                              )}
                              {config.vpn.endpoints.map((item) => (
                                <option key={item.id} value={item.id}>
                                  {item.name || item.host || "未命名"} · {item.host}:{item.port} /{" "}
                                  {item.protocol.toUpperCase()}
                                </option>
                              ))}
                            </select>
                          </Field>
                        )}
                      </div>
                      <div className="endpoint-list">
                        {config.vpn.endpoints.map((item, index) => (
                          <div
                            className="endpoint-row"
                            key={item.id}
                            onDragOver={(event) => event.preventDefault()}
                            onDrop={(event) => {
                              event.preventDefault();
                              moveEndpoint(event.dataTransfer.getData("text/plain"), index);
                            }}
                          >
                            <button
                              className="icon-button drag-handle"
                              draggable
                              aria-label={`拖动节点 ${index + 1}`}
                              onDragStart={(event) => {
                                event.dataTransfer.setData("text/plain", item.id);
                                event.dataTransfer.effectAllowed = "move";
                              }}
                            >
                              <GripVertical size={17} />
                            </button>
                            <input
                              aria-label={`节点 ${index + 1} 名称`}
                              placeholder="名称"
                              value={item.name}
                              onChange={(event) =>
                                update("vpn", {
                                  ...config.vpn,
                                  endpoints: config.vpn.endpoints.map((value) =>
                                    value.id === item.id
                                      ? { ...value, name: event.target.value }
                                      : value,
                                  ),
                                })
                              }
                            />
                            <input
                              aria-label={`节点 ${index + 1} 地址`}
                              placeholder="vpn.example.com 或 IP"
                              value={item.host}
                              onChange={(event) =>
                                update("vpn", {
                                  ...config.vpn,
                                  endpoints: config.vpn.endpoints.map((value) =>
                                    value.id === item.id
                                      ? { ...value, host: event.target.value }
                                      : value,
                                  ),
                                })
                              }
                            />
                            <input
                              aria-label={`节点 ${index + 1} 端口`}
                              type="number"
                              min={1}
                              max={65535}
                              value={item.port}
                              onChange={(event) =>
                                update("vpn", {
                                  ...config.vpn,
                                  endpoints: config.vpn.endpoints.map((value) =>
                                    value.id === item.id
                                      ? {
                                          ...value,
                                          port: Number(event.target.value),
                                        }
                                      : value,
                                  ),
                                })
                              }
                            />
                            <select
                              aria-label={`节点 ${index + 1} 协议`}
                              value={item.protocol}
                              onChange={(event) =>
                                update("vpn", {
                                  ...config.vpn,
                                  endpoints: config.vpn.endpoints.map((value) =>
                                    value.id === item.id
                                      ? {
                                          ...value,
                                          protocol: event.target.value as "udp" | "tcp",
                                        }
                                      : value,
                                  ),
                                })
                              }
                            >
                              <option value="udp">UDP</option>
                              <option value="tcp">TCP</option>
                            </select>
                            <div className="endpoint-actions">
                              <button
                                className="icon-button"
                                aria-label={`上移节点 ${index + 1}`}
                                disabled={index === 0}
                                onClick={() => moveEndpoint(item.id, index - 1)}
                              >
                                ↑
                              </button>
                              <button
                                className="icon-button"
                                aria-label={`下移节点 ${index + 1}`}
                                disabled={index === config.vpn.endpoints.length - 1}
                                onClick={() => moveEndpoint(item.id, index + 1)}
                              >
                                ↓
                              </button>
                              <button
                                className="icon-button"
                                aria-label={`删除节点 ${index + 1}`}
                                onClick={() => {
                                  const endpoints = config.vpn.endpoints.filter(
                                    (value) => value.id !== item.id,
                                  );
                                  update("vpn", {
                                    ...config.vpn,
                                    endpoints,
                                    selected_endpoint:
                                      item.id === config.vpn.selected_endpoint
                                        ? (endpoints[0]?.id ?? "")
                                        : config.vpn.selected_endpoint,
                                  });
                                }}
                              >
                                <Trash2 size={15} />
                              </button>
                            </div>
                          </div>
                        ))}
                      </div>
                      <div className="note">
                        <Network size={16} />
                        <p>
                          {config.vpn.endpoints.length === 0
                            ? "未添加节点时使用 .ovpn 中的 remote 地址。"
                            : automaticEndpoint
                              ? "低延迟节点优先；无法测速时按列表顺序连接。"
                              : "保存并连接后生效，不自动切换节点。"}
                        </p>
                      </div>
                      {status?.selected_endpoint && (
                        <p className="inline-status">
                          当前连接：
                          <code>
                            {status.selected_endpoint.ip}:{status.selected_endpoint.port} /{" "}
                            {status.selected_endpoint.protocol.toUpperCase()}
                          </code>
                        </p>
                      )}
                      {Boolean(status?.endpoint_probes.length) && (
                        <div className="table-scroll">
                          <table className="results-table">
                            <thead>
                              <tr>
                                <th>IP / 端口</th>
                                <th>协议</th>
                                <th>可用性</th>
                                <th>延迟</th>
                              </tr>
                            </thead>
                            <tbody>
                              {status?.endpoint_probes.map((item, index) => (
                                <tr key={index}>
                                  <td>
                                    <code>
                                      {item.ip}:{item.port}
                                    </code>
                                  </td>
                                  <td>{item.protocol.toUpperCase()}</td>
                                  <td title={item.error}>{item.ok ? "握手成功" : "不可用"}</td>
                                  <td>
                                    {item.latency_ms == null
                                      ? "按排序选择"
                                      : `${item.latency_ms.toFixed(1)} ms · ${item.method}`}
                                  </td>
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        </div>
                      )}
                    </div>
                    <div className="form-footer">
                      <span>将重新建立 VPN 连接</span>
                      <button
                        className="button primary"
                        disabled={busy}
                        onClick={() => control("openvpn", "restart", true)}
                      >
                        <Play size={15} />
                        保存并连接
                      </button>
                    </div>
                  </Panel>
                  <Panel
                    title="连接配置"
                    aside={
                      <>
                        <input
                          className="hidden"
                          type="file"
                          accept=".ovpn,.conf,text/plain"
                          ref={upload}
                          onChange={(event) => {
                            const file = event.target.files?.[0];
                            if (file)
                              void file.text().then((text) =>
                                update("vpn", {
                                  ...config.vpn,
                                  config: text,
                                }),
                              );
                            event.target.value = "";
                          }}
                        />
                        <button className="button small" onClick={() => upload.current?.click()}>
                          <Upload size={14} />
                          导入 .ovpn
                        </button>
                      </>
                    }
                  >
                    <div className="panel-body">
                      <Field label="OpenVPN 配置原文" help="dev、脚本、日志及管理接口由容器控制。">
                        <textarea
                          className="code-editor"
                          spellCheck={false}
                          rows={13}
                          value={config.vpn.config}
                          onChange={(event) =>
                            update("vpn", {
                              ...config.vpn,
                              config: event.target.value,
                            })
                          }
                          placeholder={
                            "client\nproto udp\nremote vpn.example.com 1194\nresolv-retry infinite\npersist-key\npersist-tun\nremote-cert-tls server\n# 在下方填入证书，或直接使用内联证书"
                          }
                        />
                      </Field>
                      <VpnConfigHelp
                        config={config.vpn.config}
                        routeMode={config.vpn.route_mode ?? "all"}
                      />
                      <div className="form-grid vpn-connection-options">
                        <Field
                          label="路由模式"
                          wide
                          help={
                            config.vpn.route_mode === "config"
                              ? "保留原文及服务器路由；隧道外出口仍受保护。"
                              : "忽略原文路由限制，全部 IPv4 流量经 VPN。"
                          }
                        >
                          <select
                            value={config.vpn.route_mode ?? "all"}
                            onChange={(event) =>
                              update("vpn", {
                                ...config.vpn,
                                route_mode: event.target.value as "all" | "config",
                              })
                            }
                          >
                            <option value="all">全部</option>
                            <option value="config">按配置</option>
                          </select>
                        </Field>
                      </div>
                      <Toggle
                        checked={config.vpn.autoconnect}
                        onChange={(autoconnect) => update("vpn", { ...config.vpn, autoconnect })}
                        label="启动时自动连接"
                      />
                      <div className="form-grid vpn-credentials">
                        <Field label="VPN 用户名（可选）">
                          <input
                            autoComplete="off"
                            value={config.vpn.username}
                            onChange={(event) =>
                              update("vpn", {
                                ...config.vpn,
                                username: event.target.value,
                              })
                            }
                          />
                        </Field>
                        <Field label="VPN 密码（可选）">
                          <input
                            type="password"
                            autoComplete="new-password"
                            value={config.vpn.password}
                            onChange={(event) =>
                              update("vpn", {
                                ...config.vpn,
                                password: event.target.value,
                              })
                            }
                          />
                        </Field>
                      </div>
                    </div>
                  </Panel>
                  <Panel title="DNS 设置">
                    <div className="panel-body">
                      <div className="form-grid compact-grid">
                        <Field label="DNS 模式" wide={automaticDns}>
                          <select
                            value={config.vpn.dns_mode}
                            onChange={(event) =>
                              update("vpn", {
                                ...config.vpn,
                                dns_mode: event.target.value as "manual" | "pushed",
                              })
                            }
                          >
                            <option value="pushed">自动 · 服务器下发</option>
                            <option value="manual">手动指定</option>
                          </select>
                        </Field>
                        {!automaticDns && (
                          <Field label="手动 DNS" help="IPv4 地址，多个地址用逗号分隔。">
                            <input
                              value={config.vpn.dns.join(", ")}
                              onChange={(event) =>
                                update("vpn", {
                                  ...config.vpn,
                                  dns: event.target.value
                                    .split(/[,，]/)
                                    .map((value) => value.trim()),
                                })
                              }
                            />
                          </Field>
                        )}
                      </div>
                      <div className="dns-status-grid">
                        <div className="dns-status-card">
                          <span>服务器下发</span>
                          <strong className="mono">
                            {status?.dns.pushed.join(", ") || "未收到"}
                          </strong>
                        </div>
                        <div className="dns-status-card">
                          <span>当前生效</span>
                          <strong className="mono">
                            {status?.dns.effective.join(", ") || "未配置"}
                          </strong>
                          <small>
                            {status?.dns.source === "manual"
                              ? "手动配置"
                              : status?.dns.source === "pushed"
                                ? "VPN 下发"
                                : "未获取"}
                          </small>
                        </div>
                      </div>
                      <div className="note">
                        <CircleHelp size={16} />
                        <p>
                          {automaticDns
                            ? status?.dns.pushed.length
                              ? "重连时自动更新，也可立即应用最近收到的 DNS。"
                              : "未收到 DNS，不会回退到公共 DNS；可改为手动指定。"
                            : "手动地址覆盖服务器下发；已有连接与缓存由应用更新。"}
                        </p>
                      </div>
                    </div>
                    <div className="form-footer">
                      <span>立即生效，无需重连</span>
                      <button
                        className="button primary"
                        disabled={busy || (automaticDns && !status?.dns.pushed.length)}
                        onClick={() =>
                          void execute(async () => {
                            await save();
                            const result = await request<{ message: string }>("/dns", {
                              method: "POST",
                            });
                            notify(result.message);
                            await refresh();
                          })
                        }
                      >
                        <Check size={15} />
                        {automaticDns
                          ? status?.dns.pushed.length
                            ? "应用服务器 DNS"
                            : "等待下发 DNS"
                          : "应用手动 DNS"}
                      </button>
                    </div>
                  </Panel>
                  <Panel
                    title="HTTPDNS 启动解析"
                    description="固定 IP 解析节点域名，首选不可用时自动切换。"
                  >
                    <div className="panel-body">
                      <div className="form-grid compact-grid">
                        <Field label="首选 HTTPDNS">
                          <select
                            value={
                              status?.httpdns_presets.find(
                                (item) => item.url === config.vpn.httpdns_url,
                              )?.id ?? "custom"
                            }
                            onChange={(event) => {
                              const preset = status?.httpdns_presets.find(
                                (candidate) => candidate.id === event.target.value,
                              );
                              if (preset)
                                update("vpn", {
                                  ...config.vpn,
                                  httpdns_url: preset.url,
                                  httpdns_ips: preset.ips,
                                  httpdns_protocol: preset.protocol,
                                });
                            }}
                          >
                            <option value="custom">自定义</option>
                            {status?.httpdns_presets.map((item) => (
                              <option key={item.id} value={item.id}>
                                {item.region} · {item.name}
                              </option>
                            ))}
                          </select>
                        </Field>
                        <Field label="协议">
                          <select
                            value={config.vpn.httpdns_protocol}
                            onChange={(event) =>
                              update("vpn", {
                                ...config.vpn,
                                httpdns_protocol: event.target.value as "doh" | "json",
                              })
                            }
                          >
                            <option value="doh">DoH · DNS wire format</option>
                            <option value="json">HTTPDNS · DNS JSON API</option>
                          </select>
                        </Field>
                        <Field label="服务 URL" wide>
                          <input
                            value={config.vpn.httpdns_url}
                            onChange={(event) =>
                              update("vpn", {
                                ...config.vpn,
                                httpdns_url: event.target.value,
                              })
                            }
                          />
                        </Field>
                        <Field
                          label="直连 IP"
                          help="多个 IPv4 用逗号分隔；TLS 仍验证服务域名证书。"
                          wide
                        >
                          <input
                            value={config.vpn.httpdns_ips.join(", ")}
                            onChange={(event) =>
                              update("vpn", {
                                ...config.vpn,
                                httpdns_ips: event.target.value
                                  .split(/[,，]/)
                                  .map((value) => value.trim()),
                              })
                            }
                          />
                        </Field>
                      </div>
                      <div className="note">
                        <Activity size={16} />
                        <p>
                          {status?.httpdns.state === "ready"
                            ? "探测完成；全部不可用时保留断线保护。"
                            : "正在探测；全部不可用时保留断线保护。"}
                        </p>
                      </div>
                      <details className="vpn-probe-results">
                        <summary>
                          启动探测结果
                          <span>{status?.httpdns.results.length ?? 0} 条</span>
                          <ChevronRight size={16} />
                        </summary>
                        <DnsTable rows={status?.httpdns.results ?? []} />
                      </details>
                    </div>
                  </Panel>
                  <Panel
                    title="证书与密钥"
                    description="内联证书无需重复填写；外部证书请粘贴完整内容。"
                    aside={<FileKey2 size={19} />}
                  >
                    <div className="panel-body certificate-grid">
                      {(
                        [
                          ["ca", "CA 证书"],
                          ["cert", "客户端证书"],
                          ["key", "客户端私钥"],
                          ["tls_auth", "tls-auth 静态密钥"],
                          ["tls_crypt", "tls-crypt 密钥"],
                        ] as const
                      ).map(([key, title]) => (
                        <details key={key} className="certificate-field">
                          <summary>
                            <span>{title}</span>
                            <span className="certificate-state">
                              {config.vpn[key].trim() ? "已填写" : "未填写"}
                            </span>
                            <ChevronRight size={16} />
                          </summary>
                          <textarea
                            aria-label={title}
                            spellCheck={false}
                            className="mono"
                            rows={5}
                            value={config.vpn[key]}
                            onChange={(event) =>
                              update("vpn", {
                                ...config.vpn,
                                [key]: event.target.value,
                              })
                            }
                            placeholder="-----BEGIN …-----"
                          />
                        </details>
                      ))}
                      <Field label="tls-auth 密钥方向">
                        <select
                          value={config.vpn.tls_auth_direction}
                          onChange={(event) =>
                            update("vpn", {
                              ...config.vpn,
                              tls_auth_direction: Number(event.target.value),
                            })
                          }
                        >
                          <option value={1}>1（通常为客户端）</option>
                          <option value={0}>0</option>
                        </select>
                      </Field>
                    </div>
                    {footer("openvpn")}
                  </Panel>
                </>
              )}
              {(tab === "http" || tab === "socks5") && (
                <Panel
                  title={tab === "http" ? "HTTP 代理配置" : "SOCKS5 代理配置"}
                  description={
                    tab === "http"
                      ? "支持 HTTP / HTTPS CONNECT，容器端口 8888。"
                      : "支持 TCP CONNECT，容器端口 1080；隧道建立后监听。"
                  }
                >
                  <div className="panel-body">
                    <Toggle
                      label={`启用${tab === "http" ? " HTTP" : " SOCKS5"} 代理`}
                      help="出口受 VPN 防火墙保护"
                      checked={config[tab].enabled}
                      onChange={(enabled) => update(tab, { ...config[tab], enabled })}
                    />
                    <div className="form-grid">
                      <AuthFields
                        key={tab}
                        value={config[tab]}
                        onChange={(value) => update(tab, value)}
                      />
                    </div>
                    <div className="note">
                      <KeyRound size={17} />
                      <p>无认证仅用于可信网络；密码认证不加密链路，建议通过 SSH 隧道访问。</p>
                    </div>
                  </div>
                  {footer(tab)}
                </Panel>
              )}
              {tab === "sshd" && (
                <Panel title="SSH 服务配置" description="容器端口 22；终端登录需单独开启。">
                  <form
                    className="panel-body"
                    autoComplete="off"
                    onSubmit={(event) => event.preventDefault()}
                  >
                    <Toggle
                      label="启用 SSH 访问"
                      checked={config.sshd.enabled}
                      onChange={(enabled) => update("sshd", { ...config.sshd, enabled })}
                      help="密码与公钥至少配置一种，也可同时启用"
                    />
                    <div className="form-grid">
                      <Field label="认证用户名">
                        <input
                          name="ssh-service-user"
                          autoComplete="off"
                          readOnly={sshCredentialFocus !== "username"}
                          onFocus={() => setSshCredentialFocus("username")}
                          onBlur={() => setSshCredentialFocus(undefined)}
                          value={config.sshd.username}
                          onChange={(event) =>
                            update("sshd", {
                              ...config.sshd,
                              username: event.target.value,
                            })
                          }
                        />
                      </Field>
                      <Field label="认证密码（可选）" help="留空关闭密码认证，改用公钥。" wide>
                        <input
                          name="ssh-service-password"
                          type="password"
                          autoComplete="new-password"
                          readOnly={sshCredentialFocus !== "password"}
                          onFocus={() => setSshCredentialFocus("password")}
                          onBlur={() => setSshCredentialFocus(undefined)}
                          value={config.sshd.password}
                          onChange={(event) =>
                            update("sshd", {
                              ...config.sshd,
                              password: event.target.value,
                            })
                          }
                        />
                      </Field>
                      <Field label="SSH 公钥（可选）" help="每行一个公钥，请勿填写私钥。" wide>
                        <textarea
                          className="mono"
                          rows={6}
                          autoComplete="off"
                          spellCheck={false}
                          value={config.sshd.authorized_keys}
                          onChange={(event) =>
                            update("sshd", {
                              ...config.sshd,
                              authorized_keys: event.target.value,
                            })
                          }
                          placeholder={
                            "ssh-ed25519 AAAA… user@laptop\nssh-ed25519 AAAA… user@phone"
                          }
                        />
                      </Field>
                    </div>
                    <Toggle
                      label="允许登录"
                      help="开启后允许终端、远程命令和 SFTP；关闭时仅允许转发。"
                      checked={config.sshd.allow_login}
                      onChange={(allow_login) => update("sshd", { ...config.sshd, allow_login })}
                    />
                    {config.sshd.allow_login && (
                      <Toggle
                        label="允许免密 sudo"
                        help="为此 SSH 用户授予容器内完整 root 权限，包括修改网络和防火墙"
                        checked={config.sshd.sudo}
                        onChange={(sudo) => update("sshd", { ...config.sshd, sudo })}
                      />
                    )}
                  </form>
                  {footer("sshd")}
                </Panel>
              )}
              {tab === "health" && (
                <>
                  <Panel
                    title="容器健康"
                    description="检查管理服务、健康任务和出口防火墙，不依赖 VPN 连接。"
                  >
                    <div className="panel-body">
                      <div className="health-status">
                        <span
                          className={`health-dot ${status?.health.container.healthy ? "healthy" : ""}`}
                        />
                        <strong>
                          {status?.health.container.healthy ? "容器运行正常" : "等待就绪或检查失败"}
                        </strong>
                      </div>
                      {status?.health.container.errors.map((error) => (
                        <p className="muted" key={error}>
                          {error}
                        </p>
                      ))}
                    </div>
                  </Panel>
                  <Panel title="OpenVPN 健康" description="未配置或手动停止时，不自动恢复。">
                    <div className="panel-body">
                      <div className="health-status">
                        <span
                          className={`health-dot ${status?.health.vpn.healthy ? "healthy" : ""}`}
                        />
                        <strong>
                          {status ? vpnHealthLabels[status.health.vpn.state] : "读取中"}
                        </strong>
                      </div>
                      {status?.health.vpn.errors.map((error) => (
                        <p className="muted" key={error}>
                          {error}
                        </p>
                      ))}
                      <p className="muted">
                        连续失败 {status?.health.vpn.consecutive_failures ?? 0} 次，达到{" "}
                        {config.health.failure_threshold} 次：
                        {healthActionLabels[config.health.action]}。
                      </p>
                      {status?.health.vpn.probe.enabled && (
                        <p className="muted">
                          隧道网络探测：
                          {status.health.vpn.probe.ok === undefined
                            ? "等待结果"
                            : status.health.vpn.probe.ok
                              ? "成功"
                              : "失败"}{" "}
                          {typeof status.health.vpn.probe.latency_ms === "number" &&
                            `· ${status.health.vpn.probe.latency_ms.toFixed(1)} ms`}
                          {status.health.vpn.probe.error && ` · ${status.health.vpn.probe.error}`}
                        </p>
                      )}
                      {status?.health.vpn.last_action && (
                        <p className="muted">
                          最近恢复：
                          {healthActionLabels[status.health.vpn.last_action.action]} ·
                          {status.health.vpn.last_action.state === "running"
                            ? "执行中"
                            : status.health.vpn.last_action.state === "success"
                              ? "已执行"
                              : "执行失败"}{" "}
                          ·
                          {new Date(
                            status.health.vpn.last_action.attempted_at * 1000,
                          ).toLocaleString()}
                          {status.health.vpn.last_action.message &&
                            ` · ${status.health.vpn.last_action.message}`}
                        </p>
                      )}
                    </div>
                  </Panel>
                  <Panel title="VPN 探测与恢复策略" description="连续失败达到阈值后执行恢复动作。">
                    <div className="panel-body">
                      <Toggle
                        label="启用隧道网络探测"
                        help="探测经 tun0；关闭后仍监测连接并按策略恢复。"
                        checked={config.health.enabled}
                        onChange={(enabled) => update("health", { ...config.health, enabled })}
                      />
                      <div className="form-grid compact-grid">
                        <Field label="不健康时的动作" wide>
                          <select
                            value={config.health.action}
                            onChange={(event) =>
                              update("health", {
                                ...config.health,
                                action: event.target.value as Config["health"]["action"],
                              })
                            }
                          >
                            {Object.entries(healthActionLabels).map(([value, label]) => (
                              <option key={value} value={value}>
                                {label}
                              </option>
                            ))}
                          </select>
                        </Field>
                        <Field label="动作冷却时间（秒）" help="两次恢复动作的最短间隔。">
                          <input
                            type="number"
                            min={1}
                            max={86400}
                            value={config.health.action_cooldown_secs}
                            onChange={(event) =>
                              update("health", {
                                ...config.health,
                                action_cooldown_secs: Number(event.target.value),
                              })
                            }
                          />
                        </Field>
                        <Field label="检查方式">
                          <select
                            value={config.health.kind}
                            onChange={(event) =>
                              update("health", {
                                ...config.health,
                                kind: event.target.value as Config["health"]["kind"],
                              })
                            }
                          >
                            <option value="ping">Ping · ICMP</option>
                            <option value="tcp">TCP · 连接端口</option>
                            <option value="http">HTTP · 状态码</option>
                          </select>
                        </Field>
                        {config.health.kind === "http" ? (
                          <>
                            <Field label="URL" wide>
                              <input
                                type="url"
                                value={config.health.url}
                                onChange={(event) =>
                                  update("health", {
                                    ...config.health,
                                    url: event.target.value,
                                  })
                                }
                              />
                            </Field>
                            <Field label="请求方法">
                              <select
                                value={config.health.http_method}
                                onChange={(event) =>
                                  update("health", {
                                    ...config.health,
                                    http_method: event.target.value as "GET" | "HEAD",
                                  })
                                }
                              >
                                <option>GET</option>
                                <option>HEAD</option>
                              </select>
                            </Field>
                            <Field label="预期状态码">
                              <input
                                type="number"
                                min={100}
                                max={599}
                                value={config.health.expected_status}
                                onChange={(event) =>
                                  update("health", {
                                    ...config.health,
                                    expected_status: Number(event.target.value),
                                  })
                                }
                              />
                            </Field>
                          </>
                        ) : (
                          <>
                            <Field label="目标域名或 IP">
                              <input
                                value={config.health.target}
                                onChange={(event) =>
                                  update("health", {
                                    ...config.health,
                                    target: event.target.value,
                                  })
                                }
                              />
                            </Field>
                            {config.health.kind === "tcp" ? (
                              <Field label="TCP 端口">
                                <input
                                  type="number"
                                  min={1}
                                  max={65535}
                                  value={config.health.port}
                                  onChange={(event) =>
                                    update("health", {
                                      ...config.health,
                                      port: Number(event.target.value),
                                    })
                                  }
                                />
                              </Field>
                            ) : (
                              <Field label="Ping 次数">
                                <input
                                  type="number"
                                  min={1}
                                  max={10}
                                  value={config.health.ping_count}
                                  onChange={(event) =>
                                    update("health", {
                                      ...config.health,
                                      ping_count: Number(event.target.value),
                                    })
                                  }
                                />
                              </Field>
                            )}
                          </>
                        )}
                        {(
                          [
                            ["interval_secs", "检查间隔（秒）", 5, 3600],
                            ["timeout_secs", "超时（秒）", 1, 30],
                            ["failure_threshold", "连续失败阈值", 1, 20],
                          ] as const
                        ).map(([key, label, min, max]) => (
                          <Field key={key} label={label}>
                            <input
                              type="number"
                              min={min}
                              max={max}
                              value={config.health[key]}
                              onChange={(event) =>
                                update("health", {
                                  ...config.health,
                                  [key]: Number(event.target.value),
                                })
                              }
                            />
                          </Field>
                        ))}
                      </div>
                    </div>
                    <div className="form-footer">
                      <span>手动检查不累计失败或触发恢复</span>
                      <button
                        className="button primary"
                        disabled={busy}
                        onClick={() =>
                          void execute(async () => {
                            await save();
                            const result = await request<VpnHealth>("/health", {
                              method: "POST",
                            });
                            setHealthTest(result);
                            await refresh();
                            notify(result.ok ? "已保存，手动检查通过" : "已保存，手动检查未通过");
                          })
                        }
                      >
                        <Activity size={15} />
                        保存并立即检查
                      </button>
                    </div>
                    {healthTest && (
                      <div className="panel-body">
                        <p className="muted">
                          本次手动测试：{vpnHealthLabels[healthTest.state]}
                          {healthTest.probe.enabled && healthTest.probe.ok !== undefined
                            ? ` · 隧道网络探测${healthTest.probe.ok ? "成功" : "失败"}`
                            : ""}
                          。{healthTest.errors.join("；")}
                        </p>
                      </div>
                    )}
                  </Panel>
                </>
              )}
              {tab === "tools" && (
                <>
                  {requestedTool && !selectedTool && <Navigate to="/tools" replace />}
                  <Panel title="切换工具">
                    <div className="panel-body">
                      <select
                        aria-label="网络工具"
                        value={selectedTool}
                        onChange={(event) =>
                          navigate(event.target.value ? `/tools/${event.target.value}` : "/tools")
                        }
                      >
                        <option value="">请选择工具</option>
                        <option value="ping">Ping · ICMP 连通性</option>
                        <option value="tcping">TCPing · TCP 端口探测</option>
                        <option value="curl">CURL · HTTP 请求与图片预览</option>
                        <option value="dns">DNS · 域名解析对比</option>
                        <option value="mtr">MTR · 持续路由探测</option>
                        <option value="traceroute">Traceroute · 路由追踪</option>
                      </select>
                    </div>
                  </Panel>
                  {(selectedTool === "tcping" || selectedTool === "curl") && (
                    <NetworkProbes key={selectedTool} tool={selectedTool} />
                  )}
                  {(selectedTool === "ping" ||
                    selectedTool === "mtr" ||
                    selectedTool === "traceroute") && (
                    <TraceProbe key={selectedTool} tool={selectedTool} />
                  )}
                  {selectedTool === "dns" && (
                    <>
                      <Panel
                        title="DNS 解析对比"
                        description="对比默认 DNS、VPN DNS 和 HTTPDNS 的地址与耗时。"
                      >
                        <form
                          className="panel-body dns-query"
                          onSubmit={(event) => {
                            event.preventDefault();
                            void execute(async () => {
                              setDnsResults(
                                await request<DnsResults>("/tools/dns", {
                                  method: "POST",
                                  body: JSON.stringify({ domain: dnsDomain }),
                                }),
                              );
                              notify("全部 DNS 测试已完成");
                            });
                          }}
                        >
                          <Field label="待解析域名" help="仅输入域名，例如 example.com。">
                            <input
                              required
                              value={dnsDomain}
                              onChange={(event) => setDnsDomain(event.target.value)}
                              placeholder="example.com"
                            />
                          </Field>
                          <button className="button primary" disabled={busy}>
                            <Network size={16} />
                            {busy ? "测试中…" : "测试全部 DNS"}
                          </button>
                        </form>
                      </Panel>
                      {dnsResults && (
                        <div className="dns-results">
                          <Panel
                            title={`默认 DNS · ${dnsResults.domain}`}
                            description="使用容器当前默认 DNS，展示实际响应的服务器与解析结果。"
                          >
                            <DnsTable rows={dnsResults.default_dns} />
                          </Panel>
                          <Panel
                            title="VPN DNS"
                            description="仅测试当前生效的 VPN DNS，未启用或尚未应用的配置不参与。"
                          >
                            <DnsTable
                              rows={dnsResults.vpn_dns}
                              emptyMessage="暂无生效的 VPN DNS，请先连接 VPN 或应用 DNS 配置。"
                            />
                          </Panel>
                          <Panel
                            title="HTTPDNS"
                            description="固定 IP 直连；按成功状态与耗时排序，比较各服务返回的地址。"
                          >
                            <DnsTable rows={dnsResults.httpdns} />
                          </Panel>
                        </div>
                      )}
                      {!dnsResults && (
                        <div className="note">
                          <CircleHelp size={18} />
                          <p>
                            默认 DNS 使用容器当前的解析设置；VPN DNS 需要隧道连接且服务器可达。
                            HTTPDNS 通过固定 IP 直连，测试不会修改当前 DNS 设置。
                          </p>
                        </div>
                      )}
                    </>
                  )}
                </>
              )}
              {tab === "logs" && (
                <Panel
                  title="日志输出"
                  description="显示最近 300 行，可手动刷新"
                  aside={
                    <div className="log-tools">
                      <select
                        aria-label="日志服务"
                        value={logService}
                        onChange={(event) => {
                          const next = new URLSearchParams(searchParams);
                          next.set("service", event.target.value);
                          setSearchParams(next);
                        }}
                      >
                        {Object.entries(names).map(([name, label]) => (
                          <option key={name} value={name}>
                            {label}
                          </option>
                        ))}
                      </select>
                      <button
                        className="button small"
                        disabled={busy}
                        onClick={() => void execute(loadLogs)}
                      >
                        <RefreshCw size={14} />
                        刷新
                      </button>
                      <button
                        className="icon-button"
                        aria-label="下载日志"
                        onClick={() => {
                          const url = URL.createObjectURL(new Blob([log], { type: "text/plain" }));
                          const anchor = document.createElement("a");
                          anchor.href = url;
                          anchor.download = `${logService}.log`;
                          anchor.click();
                          URL.revokeObjectURL(url);
                        }}
                      >
                        <ArrowDownToLine size={17} />
                      </button>
                    </div>
                  }
                >
                  <LogOutput content={log} />
                </Panel>
              )}
            </>
          )}
          <footer className="page-footer">
            <span>OpenVPN Web Console</span>
            <nav className="api-doc-links" aria-label="API 文档">
              <a href="/llms.txt" target="_blank" rel="noopener noreferrer">
                AI 接口索引
              </a>
              <a href="/llms-full.txt" target="_blank" rel="noopener noreferrer">
                完整 API 文档
              </a>
            </nav>
          </footer>
        </main>
      </div>
      <dialog
        className="modal auth-dialog"
        ref={authHelp}
        aria-labelledby="auth-help-title"
        onClick={(event) => {
          if (event.target === event.currentTarget) event.currentTarget.close();
        }}
      >
        <h2 id="auth-help-title" tabIndex={-1} autoFocus>
          设置 Basic Auth
        </h2>
        <p>
          建议在项目根目录的 <code>.env</code> 文件中设置管理用户名和密码。首次使用可复制{" "}
          <code>.env.example</code>
          ，已有文件直接编辑。
        </p>
        <pre className="logs probe-output">
          {"ADMIN_USERNAME=admin\nADMIN_PASSWORD='请替换为强密码'"}
        </pre>
        <p>保存后，在项目目录重新创建容器使配置生效：</p>
        <pre className="logs probe-output">docker compose up -d --force-recreate openvpn-web</pre>
        <p>
          使用 <code>docker run</code> 时，也可以添加以下 <code>-e</code>{" "}
          参数，并在修改后重新创建容器：
        </p>
        <pre className="logs probe-output">
          {"-e ADMIN_USERNAME=admin \\\n-e ADMIN_PASSWORD='请替换为强密码'"}
        </pre>
        <p>
          用户名和密码必须同时设置。启用后，浏览器会弹出登录框；API
          请求需要携带同一组认证信息。仅重启容器不会更新环境变量。
        </p>
        <div className="modal-actions">
          <button className="button primary" onClick={() => authHelp.current?.close()}>
            知道了
          </button>
        </div>
      </dialog>
      {newProfile && (
        <div className="modal-overlay" role="presentation">
          <section
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="new-profile-title"
          >
            <h2 id="new-profile-title">新增 VPN 配置</h2>
            <p>新配置独立保存连接、认证和健康检查。当前连接保持运行，选择新配置时才切换。</p>
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void execute(async () => {
                  if (dirty) await save();
                  const result = await request<{ config: ConfigDocument }>("/profiles", {
                    method: "POST",
                    body: JSON.stringify({ name: newProfileName }),
                  });
                  setDocumentConfig(result.config);
                  setNewProfile(false);
                  setNewProfileName("");
                  notify("配置已新增，可在右上角切换后编辑");
                });
              }}
            >
              <Field label="配置名称">
                <input
                  autoFocus
                  required
                  value={newProfileName}
                  onChange={(event) => setNewProfileName(event.target.value)}
                />
              </Field>
              <div className="modal-actions">
                <button type="button" className="button" onClick={() => setNewProfile(false)}>
                  取消
                </button>
                <button className="button primary" disabled={busy}>
                  新增配置
                </button>
              </div>
            </form>
          </section>
        </div>
      )}
      {busy && <div className="progress-bar" />}
      {notice && (
        <div
          className={`toast ${notice.error ? "error" : ""}`}
          role={notice.error ? "alert" : "status"}
        >
          {notice.error ? <CircleHelp size={18} /> : <Check size={18} />}
          <span>{notice.text}</span>
          <button aria-label="关闭提示" onClick={() => setNotice(undefined)}>
            <X size={16} />
          </button>
        </div>
      )}
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <HashRouter>
      <Routes>
        <Route path="/" element={<Navigate to="/overview" replace />} />
        {nav.map(({ id }) => (
          <Route
            key={id}
            path={id === "tools" ? "/tools/:tool?" : `/${id}`}
            element={<App tab={id} />}
          />
        ))}
        <Route path="*" element={<Navigate to="/overview" replace />} />
      </Routes>
    </HashRouter>
  </StrictMode>,
);
