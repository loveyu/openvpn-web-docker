import {
  StrictMode,
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createRoot } from "react-dom/client";
import {
  HashRouter,
  NavLink,
  Navigate,
  Route,
  Routes,
  useNavigate,
  useParams,
} from "react-router-dom";
import { request } from "./api";
import { NetworkProbes } from "./NetworkProbes";
import { TraceProbe } from "./TraceProbe";
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

type Tab =
  "overview" | "vpn" | "http" | "socks5" | "sshd" | "logs" | "health" | "tools";
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
  overview: "所有连接，一个控制台。查看隧道与各项服务的实时状态。",
  vpn: "导入连接配置，管理凭据与证书，建立你的专属网络通道。",
  http: "为浏览器与应用提供经过 VPN 隧道的 HTTP / HTTPS 代理。",
  socks5: "按需开启 SOCKS5 TCP 代理，支持无认证或用户名密码认证。",
  sshd: "按需启用安全终端和端口转发，支持密码与公钥登录。",
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
              ...(authentication === "none"
                ? { username: "", password: "" }
                : {}),
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
              onChange={(event) =>
                onChange({ ...value, username: event.target.value })
              }
              placeholder="proxyuser"
            />
          </Field>
          <Field label="密码">
            <input
              type="password"
              autoComplete="new-password"
              value={value.password}
              onChange={(event) =>
                onChange({ ...value, password: event.target.value })
              }
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

function DnsTable({ rows }: { rows: DnsResult[] }) {
  return (
    <div className="table-scroll">
      <table className="results-table">
        <thead>
          <tr>
            <th>服务</th>
            <th>直连 IP</th>
            <th>状态 / 耗时</th>
            <th>解析结果</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row, index) => (
            <tr key={`${row.provider}-${row.ip}-${index}`}>
              <td>{row.provider}</td>
              <td>
                <code>{row.ip}</code>
              </td>
              <td>
                <span className={row.ok ? "mint" : "muted"}>
                  {row.ok ? `${row.latency_ms.toFixed(1)} ms` : "失败"}
                </span>
              </td>
              <td>
                <code>{row.ok ? row.answers.join(", ") : row.error}</code>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {rows.length === 0 && (
        <p className="table-empty">探测中，后台异步更新结果…</p>
      )}
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

  return (
    <div className="theme-switcher" role="group" aria-label="外观模式">
      {options.map(({ value, label, icon: Icon }) => (
        <button
          key={value}
          type="button"
          title={label}
          aria-label={label}
          aria-pressed={theme === value}
          onClick={() => {
            setTheme(value);
            document.documentElement.dataset.theme = value;
            try {
              localStorage.setItem("vpnweb-theme", value);
            } catch {
              // 本地存储不可用时，当前页面的主题选择仍然生效。
            }
          }}
        >
          <Icon size={18} />
        </button>
      ))}
    </div>
  );
}

function App({ tab }: { tab: Tab }) {
  const navigate = useNavigate();
  const { tool: requestedTool = "" } = useParams();
  const selectedTool = [
    "ping",
    "tcping",
    "curl",
    "dns",
    "mtr",
    "traceroute",
  ].includes(requestedTool)
    ? requestedTool
    : "";
  const setTab = (value: Tab) => navigate(`/${value}`);
  const [documentConfig, setDocumentConfig] = useState<ConfigDocument>();
  const profile = documentConfig?.profiles.find(
    (item) => item.id === documentConfig.active_profile,
  );
  const config = profile?.settings;
  const [newProfile, setNewProfile] = useState(false);
  const authHelp = useRef<HTMLDialogElement>(null);
  // 部署配置不是登录表单，仅在获焦时开放编辑，避免浏览器回填管理凭据。
  const [sshCredentialFocus, setSshCredentialFocus] = useState<
    "username" | "password"
  >();
  const [newProfileName, setNewProfileName] = useState("");
  const [dnsDomain, setDnsDomain] = useState("example.com");
  const [dnsResults, setDnsResults] = useState<DnsResults>();
  const [status, setStatus] = useState<Status>();
  const [healthTest, setHealthTest] = useState<VpnHealth>();
  useEffect(() => setHealthTest(undefined), [documentConfig?.active_profile]);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<{ text: string; error?: boolean }>();
  const [logService, setLogService] = useState<Service>("openvpn");
  const [log, setLog] = useState("");
  const upload = useRef<HTMLInputElement>(null);

  const notify = useCallback(
    (text: string, error = false) => setNotice({ text, error }),
    [],
  );
  const refresh = useCallback(
    async () => setStatus(await request<Status>("/status")),
    [],
  );
  useEffect(() => {
    let cancelled = false;
    Promise.all([
      request<ConfigDocument>("/config"),
      request<Status>("/status"),
    ])
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
      if (!document.hidden)
        refresh().catch((error) => notify(error.message, true));
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
    const response = await request<{ content: string }>(
      `/logs/${logService}?lines=300`,
    );
    setLog(response.content);
  }, [logService]);
  useEffect(() => {
    if (tab === "logs")
      loadLogs().catch((error) => notify(error.message, true));
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
  function control(
    service: Service,
    action: "start" | "stop" | "restart",
    saveFirst = false,
  ) {
    void execute(async () => {
      if (saveFirst || (service === "openvpn" && action !== "stop"))
        await save();
      const result = await request<{ message: string }>(
        `/services/${service}/${action}`,
        { method: "POST" },
      );
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
        保存后重启此服务以应用配置
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
    <div className="app-shell">
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
        <nav aria-label="主导航">
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
              className="button small"
              disabled={busy}
              onClick={() => setNewProfile(true)}
            >
              <Plus size={14} />
              新增配置
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
            {tab !== "tools" && (
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
              <button
                className="button"
                onClick={() => window.location.reload()}
              >
                重试
              </button>
            </div>
          ) : (
            <>
              {tab === "overview" && (
                <>
                  <section
                    className={`connection-hero ${connected ? "connected" : ""}`}
                  >
                    <div className="hero-main">
                      <div className="hero-shield">
                        <ShieldCheck size={32} />
                      </div>
                      <div>
                        <div className="hero-kicker">
                          OPENVPN TUNNEL{" "}
                          <span>{connected ? "已连接" : "未连接"}</span>
                        </div>
                        <h2>
                          {connected
                            ? "安全通道已建立"
                            : "准备好，连接你的网络。"}
                        </h2>
                        <p>
                          {connected
                            ? `隧道地址 ${status?.vpn.ip} · 所有代理出口均通过 VPN`
                            : "添加 OpenVPN 配置和证书，开启安全的代理与远程访问。"}
                        </p>
                      </div>
                    </div>
                    <button
                      className="button hero-button"
                      disabled={busy}
                      onClick={() =>
                        connected
                          ? control("openvpn", "stop")
                          : config.vpn.config.trim() ||
                              config.vpn.endpoints.length > 0
                            ? control("openvpn", "start")
                            : setTab("vpn")
                      }
                    >
                      {connected ? (
                        <Square size={15} />
                      ) : (
                        <ArrowRight size={16} />
                      )}
                      {connected
                        ? "断开连接"
                        : config.vpn.config.trim() ||
                            config.vpn.endpoints.length > 0
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
                      <p>由 Supervisor 统一守护</p>
                    </div>
                    <div className="stat">
                      <div className="stat-label">
                        <ShieldCheck size={16} />
                        出口保护
                      </div>
                      <strong className="mint">
                        {status?.kill_switch ? "已开启" : "读取中"}
                      </strong>
                      <p>VPN 断开后阻止流量直连</p>
                    </div>
                    <div className="stat">
                      <div className="stat-label">
                        <Network size={16} />
                        隧道 DNS
                      </div>
                      <strong className="mono dns-stat">
                        {status?.dns.effective[0] ?? "未获取"}
                      </strong>
                      <p>DNS 请求通过 VPN 发送</p>
                    </div>
                  </div>
                  <Panel
                    title="服务状态"
                    description="独立控制每项服务的生命周期"
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
                      {(
                        [
                          "openvpn",
                          "http",
                          "socks5",
                          "sshd",
                          "backend",
                        ] as Service[]
                      ).map((name, index) => {
                        const Icon = [
                          ShieldCheck,
                          Globe2,
                          Network,
                          Terminal,
                          Settings2,
                        ][index];
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
                              <span>
                                {
                                  [
                                    "加密隧道",
                                    "Tinyproxy",
                                    "Dante · TCP CONNECT",
                                    "OpenSSH",
                                    "Rust · Axum",
                                  ][index]
                                }
                              </span>
                            </div>
                            <code>{port}</code>
                            <Badge state={serviceState(name)} />
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
                                      serviceState(name) === "RUNNING"
                                        ? "stop"
                                        : "start",
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
                                className="icon-button"
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
                                  onClick={() =>
                                    setTab(name === "openvpn" ? "vpn" : name)
                                  }
                                >
                                  <Settings2 size={15} />
                                </button>
                              )}
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  </Panel>
                  <Panel
                    title="当前监听端口"
                    description="自动检测各服务的容器内监听端口；服务未监听时显示预设端口。宿主机映射由部署配置提供。"
                  >
                    <div className="port-grid">
                      {status?.ports.map((item) => {
                        return (
                          <div className="port-card" key={item.name}>
                            <span>{names[item.name]}</span>
                            <span className="port-label">容器端口</span>
                            <strong className="mono">:{item.port}</strong>
                            <small
                              className={item.listening ? "mint" : "muted"}
                            >
                              {item.listening ? "正在监听" : "未监听"}
                            </small>
                            <div className="port-mapping">
                              {item.published_port ? (
                                <>
                                  <span>宿主机映射</span>
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
                  <div className="note">
                    <ShieldCheck size={17} />
                    <p>
                      容器健康：
                      {status?.health.container.healthy
                        ? "正常"
                        : "待就绪 / 异常"}
                      。 OpenVPN 健康：
                      {status
                        ? vpnHealthLabels[status.health.vpn.state]
                        : "读取中"}
                      。{status?.health.container.errors.join("；")}
                      <button
                        className="text-button"
                        onClick={() => setTab("health")}
                      >
                        查看健康检查
                      </button>
                    </p>
                  </div>
                  <div className="note">
                    <LockKeyhole size={17} />
                    <p>
                      HTTP、SOCKS5 和 SSH 转发共享 VPN
                      网络出口。关闭隧道后，管理页面和已发布的 SSH
                      端口仍可访问。
                    </p>
                  </div>
                </>
              )}
              {tab === "vpn" && (
                <>
                  <div className="note">
                    <Activity size={17} />
                    <p>
                      OpenVPN 健康：
                      {status
                        ? vpnHealthLabels[status.health.vpn.state]
                        : "读取中"}
                      。{status?.health.vpn.errors.join("；")}
                    </p>
                  </div>
                  <Panel
                    title="Endpoint 连接选择"
                    description="默认 Auto 自动探测；支持域名与 IPv4。拖拽调整优先级，无延迟结果时按列表顺序选择。"
                    aside={
                      <button
                        className="button small"
                        onClick={() => {
                          const id =
                            globalThis.crypto?.randomUUID?.() ??
                            `endpoint-${Date.now()}-${Math.random().toString(16).slice(2)}`;
                          update("vpn", {
                            ...config.vpn,
                            selected_endpoint:
                              config.vpn.selected_endpoint || id,
                            endpoints: [
                              ...config.vpn.endpoints,
                              {
                                id,
                                name: "",
                                host: "",
                                port: 1194,
                                protocol: "udp",
                              },
                            ],
                          });
                        }}
                      >
                        <Plus size={14} />
                        添加 endpoint
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
                        <Field label="选择模式">
                          <select
                            value={config.vpn.selection_mode}
                            onChange={(event) =>
                              update("vpn", {
                                ...config.vpn,
                                selection_mode: event.target.value as
                                  "auto" | "manual",
                              })
                            }
                          >
                            <option value="auto">Auto · 自动探测与选择</option>
                            <option value="manual">手动选择</option>
                          </select>
                        </Field>
                        <Field label="Endpoint（记住上次使用）" wide>
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
                              <option value="">
                                使用 .ovpn 原文中的 remote
                              </option>
                            )}
                            {config.vpn.endpoints.map((item) => (
                              <option key={item.id} value={item.id}>
                                {item.name || item.host || "未命名"} ·{" "}
                                {item.host}:{item.port} /{" "}
                                {item.protocol.toUpperCase()}
                              </option>
                            ))}
                          </select>
                        </Field>
                      </div>
                      <div className="endpoint-list">
                        {config.vpn.endpoints.map((item, index) => (
                          <div
                            className="endpoint-row"
                            key={item.id}
                            onDragOver={(event) => event.preventDefault()}
                            onDrop={(event) => {
                              event.preventDefault();
                              moveEndpoint(
                                event.dataTransfer.getData("text/plain"),
                                index,
                              );
                            }}
                          >
                            <button
                              className="icon-button drag-handle"
                              draggable
                              aria-label={`拖动 endpoint ${index + 1}`}
                              onDragStart={(event) => {
                                event.dataTransfer.setData(
                                  "text/plain",
                                  item.id,
                                );
                                event.dataTransfer.effectAllowed = "move";
                              }}
                            >
                              <GripVertical size={17} />
                            </button>
                            <input
                              aria-label={`Endpoint ${index + 1} 名称`}
                              placeholder="名称"
                              value={item.name}
                              onChange={(event) =>
                                update("vpn", {
                                  ...config.vpn,
                                  endpoints: config.vpn.endpoints.map(
                                    (value) =>
                                      value.id === item.id
                                        ? { ...value, name: event.target.value }
                                        : value,
                                  ),
                                })
                              }
                            />
                            <input
                              aria-label={`Endpoint ${index + 1} 地址`}
                              placeholder="vpn.example.com 或 IP"
                              value={item.host}
                              onChange={(event) =>
                                update("vpn", {
                                  ...config.vpn,
                                  endpoints: config.vpn.endpoints.map(
                                    (value) =>
                                      value.id === item.id
                                        ? { ...value, host: event.target.value }
                                        : value,
                                  ),
                                })
                              }
                            />
                            <input
                              aria-label={`Endpoint ${index + 1} 端口`}
                              type="number"
                              min={1}
                              max={65535}
                              value={item.port}
                              onChange={(event) =>
                                update("vpn", {
                                  ...config.vpn,
                                  endpoints: config.vpn.endpoints.map(
                                    (value) =>
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
                              aria-label={`Endpoint ${index + 1} 协议`}
                              value={item.protocol}
                              onChange={(event) =>
                                update("vpn", {
                                  ...config.vpn,
                                  endpoints: config.vpn.endpoints.map(
                                    (value) =>
                                      value.id === item.id
                                        ? {
                                            ...value,
                                            protocol: event.target.value as
                                              "udp" | "tcp",
                                          }
                                        : value,
                                  ),
                                })
                              }
                            >
                              <option value="udp">UDP</option>
                              <option value="tcp">TCP</option>
                            </select>
                            <button
                              className="icon-button"
                              aria-label={`上移 endpoint ${index + 1}`}
                              disabled={index === 0}
                              onClick={() => moveEndpoint(item.id, index - 1)}
                            >
                              ↑
                            </button>
                            <button
                              className="icon-button"
                              aria-label={`下移 endpoint ${index + 1}`}
                              disabled={
                                index === config.vpn.endpoints.length - 1
                              }
                              onClick={() => moveEndpoint(item.id, index + 1)}
                            >
                              ↓
                            </button>
                            <button
                              className="icon-button"
                              aria-label={`删除 endpoint ${index + 1}`}
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
                        ))}
                      </div>
                      <div className="note">
                        <Network size={16} />
                        <p>
                          Auto 对 IP＋端口执行 OpenVPN 握手验证（含 UDP），再按
                          Ping / TCPing
                          延迟选择；无法测得延迟时按排序选首个可用项。手动选择时仅连接指定项。填写列表后，原文中的
                          remote 将由列表接管。
                        </p>
                      </div>
                      {status?.selected_endpoint && (
                        <p className="inline-status">
                          当前连接：
                          <code>
                            {status.selected_endpoint.ip}:
                            {status.selected_endpoint.port} /{" "}
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
                                  <td title={item.error}>
                                    {item.ok ? "握手成功" : "不可用"}
                                  </td>
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
                      <span>切换配置与自动探测会重新建立 VPN 连接</span>
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
                    description="支持标准 .ovpn 文件、内联证书与手动 OpenVPN 参数"
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
                        <button
                          className="button small"
                          onClick={() => upload.current?.click()}
                        >
                          <Upload size={14} />
                          导入 .ovpn
                        </button>
                      </>
                    }
                  >
                    <div className="panel-body">
                      <Field
                        label="OpenVPN 配置原文"
                        help="保留 remote、加密与验证选项；dev、脚本、日志与管理接口由容器控制。所有代理使用全隧道 IPv4 出口。"
                      >
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
                      <Toggle
                        checked={config.vpn.autoconnect}
                        onChange={(autoconnect) =>
                          update("vpn", { ...config.vpn, autoconnect })
                        }
                        label="容器启动时自动连接"
                        help="仅在已填写有效 OpenVPN 配置时建立连接"
                      />
                      <div className="form-grid">
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
                  <Panel
                    title="DNS 设置"
                    description="连接与重连时读取 OpenVPN 下发的 DNS，支持手动覆盖与热切换"
                  >
                    <div className="panel-body">
                      <div className="form-grid compact-grid">
                        <Field label="DNS 模式">
                          <select
                            value={config.vpn.dns_mode}
                            onChange={(event) =>
                              update("vpn", {
                                ...config.vpn,
                                dns_mode: event.target.value as
                                  "manual" | "pushed",
                              })
                            }
                          >
                            <option value="pushed">
                              自动 · 使用 OpenVPN 下发 DNS
                            </option>
                            <option value="manual">手动指定</option>
                          </select>
                        </Field>
                        <Field
                          label="手动 DNS"
                          help="IPv4 地址，多个地址用逗号分隔。"
                        >
                          <input
                            disabled={config.vpn.dns_mode !== "manual"}
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
                      </div>
                      <dl className="detail-list">
                        <div>
                          <dt>服务器下发</dt>
                          <dd>
                            <code>
                              {status?.dns.pushed.join(", ") || "尚未获取"}
                            </code>
                          </dd>
                        </div>
                        <div>
                          <dt>当前生效</dt>
                          <dd>
                            <code>
                              {status?.dns.effective.join(", ") ||
                                "暂无可用 DNS"}
                            </code>
                            <span>
                              {status?.dns.source === "manual"
                                ? "手动配置"
                                : status?.dns.source === "pushed"
                                  ? "VPN 下发"
                                  : "未获取"}
                            </span>
                          </dd>
                        </div>
                      </dl>
                      <div className="note">
                        <CircleHelp size={16} />
                        <p>
                          自动模式未收到服务器 DNS 时不会回退到公网 DNS。仅应用
                          DNS 无需重启容器、VPN
                          或代理；已有连接与缓存由应用自行更新。
                        </p>
                      </div>
                    </div>
                    <div className="form-footer">
                      <span>连接状态与解析结果可在网络工具中检查</span>
                      <button
                        className="button primary"
                        disabled={busy}
                        onClick={() =>
                          void execute(async () => {
                            await save();
                            const result = await request<{ message: string }>(
                              "/dns",
                              { method: "POST" },
                            );
                            notify(result.message);
                            await refresh();
                          })
                        }
                      >
                        <Check size={15} />
                        仅应用 DNS
                      </button>
                    </div>
                  </Panel>
                  <Panel
                    title="HTTPDNS 启动解析"
                    description="通过固定 IP 直连，不依赖系统 DNS。首选不可用时自动回退到可用服务。"
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
                              const item = status?.httpdns_presets.find(
                                (item) => item.id === event.target.value,
                              );
                              if (item)
                                update("vpn", {
                                  ...config.vpn,
                                  httpdns_url: item.url,
                                  httpdns_ips: item.ips,
                                  httpdns_protocol: item.protocol,
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
                                httpdns_protocol: event.target.value as
                                  "doh" | "json",
                              })
                            }
                          >
                            <option value="doh">DoH · DNS wire format</option>
                            <option value="json">HTTPDNS · DNS JSON API</option>
                          </select>
                        </Field>
                        <Field label="服务 URL">
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
                            ? "启动探测已完成，以下结果按耗时排序。"
                            : "启动探测正在后台异步进行，不阻塞管理页面。"}
                          默认首选腾讯
                          DNSPod；均不可用时会报告解析失败并保留断线保护。
                        </p>
                      </div>
                      <DnsTable rows={status?.httpdns.results ?? []} />
                    </div>
                  </Panel>
                  <Panel
                    title="证书与密钥"
                    description="已在 .ovpn 内联的证书无需重复填写；外部文件请粘贴完整内容"
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
                        <Field key={key} label={title}>
                          <textarea
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
                        </Field>
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
                      ? "由 Tinyproxy 提供 HTTP 和 HTTPS CONNECT 代理，容器端口固定为 8888"
                      : "由 Dante 提供 TCP CONNECT 代理，容器端口固定为 1080，等待 tun0 建立后开始监听"
                  }
                >
                  <div className="panel-body">
                    <Toggle
                      label={`启用${tab === "http" ? " HTTP" : " SOCKS5"} 代理`}
                      help="启用后由 Supervisor 守护，出口受 VPN 防火墙保护"
                      checked={config[tab].enabled}
                      onChange={(enabled) =>
                        update(tab, { ...config[tab], enabled })
                      }
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
                      <p>
                        无认证模式适合可信网络。用户名密码认证本身不加密客户端到代理的链路，请通过可信网络或
                        SSH 隧道访问。
                      </p>
                    </div>
                  </div>
                  {footer(tab)}
                </Panel>
              )}
              {tab === "sshd" && (
                <Panel
                  title="SSH 服务配置"
                  description="支持终端、SFTP、TCP 转发与 TUN 转发；容器端口固定为 22，默认关闭"
                >
                  <form
                    className="panel-body"
                    autoComplete="off"
                    onSubmit={(event) => event.preventDefault()}
                  >
                    <Toggle
                      label="启用 SSH 访问"
                      checked={config.sshd.enabled}
                      onChange={(enabled) =>
                        update("sshd", { ...config.sshd, enabled })
                      }
                      help="密码与公钥至少配置一种，也可同时启用"
                    />
                    <div className="form-grid">
                      <Field label="登录用户名">
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
                      <Field
                        label="登录密码（可选）"
                        help="留空关闭密码登录，改用公钥。"
                        wide
                      >
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
                      <Field
                        label="SSH 公钥（支持多个，可选）"
                        help="可粘贴多个公钥，每行一个，均可用于此用户登录。请勿填写私钥。"
                        wide
                      >
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
                      label="允许免密 sudo"
                      help="为此 SSH 用户授予容器内完整 root 权限，包括修改网络和防火墙"
                      checked={config.sshd.sudo}
                      onChange={(sudo) =>
                        update("sshd", { ...config.sshd, sudo })
                      }
                    />
                  </form>
                  {footer("sshd")}
                </Panel>
              )}
              {tab === "health" && (
                <>
                  <Panel
                    title="容器健康"
                    description="Docker 使用 /healthz 检查管理服务、健康检查任务和出口防火墙，与 VPN 连接状态独立。"
                  >
                    <div className="panel-body">
                      <div className="health-status">
                        <span
                          className={`health-dot ${status?.health.container.healthy ? "healthy" : ""}`}
                        />
                        <strong>
                          {status?.health.container.healthy
                            ? "容器运行正常"
                            : "等待就绪或检查失败"}
                        </strong>
                      </div>
                      {status?.health.container.errors.map((error) => (
                        <p className="muted" key={error}>
                          {error}
                        </p>
                      ))}
                    </div>
                  </Panel>
                  <Panel
                    title="OpenVPN 健康"
                    description="检查隧道连接与网络可用性；未配置或手动停止时不执行自动恢复。"
                  >
                    <div className="panel-body">
                      <div className="health-status">
                        <span
                          className={`health-dot ${status?.health.vpn.healthy ? "healthy" : ""}`}
                        />
                        <strong>
                          {status
                            ? vpnHealthLabels[status.health.vpn.state]
                            : "读取中"}
                        </strong>
                      </div>
                      {status?.health.vpn.errors.map((error) => (
                        <p className="muted" key={error}>
                          {error}
                        </p>
                      ))}
                      <p className="muted">
                        连续失败 {status?.health.vpn.consecutive_failures ?? 0}{" "}
                        次，达到 {config.health.failure_threshold} 次：
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
                          {typeof status.health.vpn.probe.latency_ms ===
                            "number" &&
                            `· ${status.health.vpn.probe.latency_ms.toFixed(1)} ms`}
                          {status.health.vpn.probe.error &&
                            ` · ${status.health.vpn.probe.error}`}
                        </p>
                      )}
                      {status?.health.vpn.last_action && (
                        <p className="muted">
                          最近恢复：
                          {
                            healthActionLabels[
                              status.health.vpn.last_action.action
                            ]
                          }{" "}
                          ·
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
                  <Panel
                    title="VPN 探测与恢复策略"
                    description="连接故障或隧道网络探测连续失败达到阈值后执行所选动作；默认失败 3 次重启 OpenVPN 服务。"
                  >
                    <div className="panel-body">
                      <Toggle
                        label="启用隧道网络探测"
                        help="Ping、TCP 和 HTTP 请求固定通过 tun0。关闭后仍检查 VPN 连接状态并按策略恢复。"
                        checked={config.health.enabled}
                        onChange={(enabled) =>
                          update("health", { ...config.health, enabled })
                        }
                      />
                      <div className="form-grid compact-grid">
                        <Field label="不健康时的动作" wide>
                          <select
                            value={config.health.action}
                            onChange={(event) =>
                              update("health", {
                                ...config.health,
                                action: event.target
                                  .value as Config["health"]["action"],
                              })
                            }
                          >
                            {Object.entries(healthActionLabels).map(
                              ([value, label]) => (
                                <option key={value} value={value}>
                                  {label}
                                </option>
                              ),
                            )}
                          </select>
                        </Field>
                        <Field
                          label="动作冷却时间（秒）"
                          help="两次恢复动作的最短间隔，防止持续故障时频繁重连。"
                        >
                          <input
                            type="number"
                            min={1}
                            max={86400}
                            value={config.health.action_cooldown_secs}
                            onChange={(event) =>
                              update("health", {
                                ...config.health,
                                action_cooldown_secs: Number(
                                  event.target.value,
                                ),
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
                                kind: event.target
                                  .value as Config["health"]["kind"],
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
                            <Field label="URL">
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
                                    http_method: event.target.value as
                                      "GET" | "HEAD",
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
                      <span>手动测试只返回结果，不累计失败或触发恢复</span>
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
                            notify(
                              result.ok
                                ? "已保存，手动检查通过"
                                : "已保存，手动检查未通过",
                            );
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
                          {healthTest.probe.enabled &&
                          healthTest.probe.ok !== undefined
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
                  {requestedTool && !selectedTool && (
                    <Navigate to="/tools" replace />
                  )}
                  <Panel
                    title="选择网络工具"
                    description="选择要使用的工具，开始诊断容器网络。"
                  >
                    <div className="panel-body">
                      <Field label="网络工具">
                        <select
                          value={selectedTool}
                          onChange={(event) =>
                            navigate(
                              event.target.value
                                ? `/tools/${event.target.value}`
                                : "/tools",
                            )
                          }
                        >
                          <option value="">请选择工具</option>
                          <option value="ping">Ping · ICMP 连通性</option>
                          <option value="tcping">TCPing · TCP 端口探测</option>
                          <option value="curl">
                            CURL · HTTP 请求与图片预览
                          </option>
                          <option value="dns">DNS · 域名解析对比</option>
                          <option value="mtr">MTR · 持续路由探测</option>
                          <option value="traceroute">
                            Traceroute · 路由追踪
                          </option>
                        </select>
                      </Field>
                    </div>
                  </Panel>
                  {(selectedTool === "ping" ||
                    selectedTool === "tcping" ||
                    selectedTool === "curl") && (
                    <NetworkProbes key={selectedTool} tool={selectedTool} />
                  )}
                  {(selectedTool === "mtr" ||
                    selectedTool === "traceroute") && (
                    <TraceProbe key={selectedTool} tool={selectedTool} />
                  )}
                  {selectedTool === "dns" && (
                    <>
                      <Panel
                        title="DNS 解析对比"
                        description="并行测试全部预设 HTTPDNS、自定义 HTTPDNS、VPN 下发和手动配置的 DNS；展示每个直连 IP 的结果。"
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
                          <Field
                            label="待解析域名"
                            help="仅输入域名，例如 example.com。"
                          >
                            <input
                              required
                              value={dnsDomain}
                              onChange={(event) =>
                                setDnsDomain(event.target.value)
                              }
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
                        <>
                          <Panel
                            title={`HTTPDNS · ${dnsResults.domain}`}
                            description="固定 IP 直连；按耗时展示，可直接比较成功、失败与返回地址。"
                          >
                            <DnsTable rows={dnsResults.httpdns} />
                          </Panel>
                          <Panel
                            title="VPN DNS"
                            description="包含服务器下发、手动指定与当前生效的 DNS；失败时显示具体原因。"
                          >
                            <DnsTable rows={dnsResults.vpn_dns} />
                          </Panel>
                        </>
                      )}
                      {!dnsResults && (
                        <div className="note">
                          <CircleHelp size={18} />
                          <p>
                            HTTPDNS 不依赖容器 DNS。VPN DNS
                            需要隧道连接且服务器可达，测试不会修改当前 DNS
                            设置。
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
                        onChange={(event) =>
                          setLogService(event.target.value as Service)
                        }
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
                          const url = URL.createObjectURL(
                            new Blob([log], { type: "text/plain" }),
                          );
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
                  <pre className="logs">
                    {log || "暂无日志。启动服务后在这里查看输出。"}
                  </pre>
                </Panel>
              )}
            </>
          )}
          <footer className="page-footer">
            <span>OpenVPN Web Console</span>
            <span>私有部署 · 自由连接</span>
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
          建议在项目根目录的 <code>.env</code>{" "}
          文件中设置管理用户名和密码。首次使用可复制 <code>.env.example</code>
          ，已有文件直接编辑。
        </p>
        <pre className="logs probe-output">
          {"ADMIN_USERNAME=admin\nADMIN_PASSWORD='请替换为强密码'"}
        </pre>
        <p>保存后，在项目目录重新创建容器使配置生效：</p>
        <pre className="logs probe-output">
          docker compose up -d --force-recreate openvpn-web
        </pre>
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
          <button
            className="button primary"
            onClick={() => authHelp.current?.close()}
          >
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
            <p>
              新配置独立保存连接、认证和健康检查。当前连接保持运行，选择新配置时才切换。
            </p>
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void execute(async () => {
                  if (dirty) await save();
                  const result = await request<{ config: ConfigDocument }>(
                    "/profiles",
                    {
                      method: "POST",
                      body: JSON.stringify({ name: newProfileName }),
                    },
                  );
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
                <button
                  type="button"
                  className="button"
                  onClick={() => setNewProfile(false)}
                >
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
