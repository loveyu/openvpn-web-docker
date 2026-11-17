export interface ProxyConfig {
  enabled: boolean;
  username: string;
  password: string;
  authentication: "none" | "password";
}
export interface Config {
  vpn: {
    config: string;
    endpoints: {
      id: string;
      name: string;
      host: string;
      port: number;
      protocol: "udp" | "tcp";
    }[];
    selected_endpoint: string;
    selection_mode: "auto" | "manual";
    username: string;
    password: string;
    ca: string;
    cert: string;
    key: string;
    tls_auth: string;
    tls_crypt: string;
    tls_auth_direction: number;
    dns: string[];
    dns_mode: "manual" | "pushed";
    httpdns_url: string;
    httpdns_ips: string[];
    httpdns_protocol: "doh" | "json";
    autoconnect: boolean;
  };
  http: ProxyConfig;
  socks5: ProxyConfig;
  sshd: Omit<ProxyConfig, "authentication"> & {
    authorized_keys: string;
    sudo: boolean;
  };
  health: {
    enabled: boolean;
    kind: "ping" | "tcp" | "http";
    target: string;
    port: number;
    url: string;
    http_method: "GET" | "HEAD";
    expected_status: number;
    interval_secs: number;
    timeout_secs: number;
    failure_threshold: number;
    ping_count: number;
    action: "none" | "reconnect" | "restart" | "reapply";
    action_cooldown_secs: number;
  };
}
export interface Profile {
  id: string;
  name: string;
  settings: Config;
}
export interface ConfigDocument {
  active_profile: string;
  profiles: Profile[];
}
export interface DnsResult {
  provider: string;
  provider_id?: string;
  ip: string;
  url?: string;
  protocol?: string;
  ok: boolean;
  answers: string[];
  latency_ms: number;
  error?: string;
}
export interface DnsResults {
  domain: string;
  httpdns: DnsResult[];
  vpn_dns: DnsResult[];
}
export type Service = "openvpn" | "http" | "socks5" | "sshd" | "backend";
export interface Status {
  services: { name: string; state: string; pid: number; description: string }[];
  vpn: { state: string; ip: string; detail?: string; since?: number };
  version: string;
  kill_switch: boolean;
  basic_auth_enabled: boolean;
  ports: {
    name: Service;
    port: number;
    listening: boolean;
    published_port: number | null;
    target_port: number;
  }[];
  dns: {
    mode: string;
    pushed: string[];
    effective: string[];
    source: string;
    detected_at?: number;
  };
  remote_endpoints: { ip: string; port: number; protocol: string }[];
  httpdns: { state: string; results: DnsResult[] };
  httpdns_presets: {
    id: string;
    name: string;
    region: string;
    url: string;
    ips: string[];
    protocol: "doh" | "json";
  }[];
  httpdns_current?: DnsResult;
  selected_endpoint: {
    ip: string;
    port: number;
    protocol: string;
    endpoint_id: string;
  } | null;
  endpoint_probes: {
    ip: string;
    port: number;
    protocol: string;
    endpoint_id: string;
    ok: boolean;
    latency_ms: number | null;
    method?: string;
    error?: string;
  }[];
  health: {
    container: { healthy: boolean; errors: string[] };
    vpn: VpnHealth & {
      consecutive_failures: number;
      last_action: {
        action: Config["health"]["action"];
        attempted_at: number;
        state: "running" | "success" | "failed";
        message?: string;
      } | null;
    };
  };
  active_profile: string;
}

export interface VpnHealth {
  state: "unconfigured" | "stopped" | "connecting" | "checking" | "healthy" | "unhealthy";
  ok: boolean;
  healthy: boolean | null;
  errors: string[];
  probe: {
    enabled: boolean;
    ok?: boolean;
    target?: string;
    latency_ms?: number;
    error?: string;
    checked_at?: number;
  };
}
