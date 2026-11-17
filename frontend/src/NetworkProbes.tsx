import { useEffect, useState } from "react";
import { Globe2, Network } from "lucide-react";
import { request } from "./api";
import { useToolPreference } from "./useToolPreference";

interface ProbeResult {
  target: string;
  ip?: string;
  ok: boolean;
  elapsed_ms: number;
  stdout: string;
  stderr: string;
  exit_code: number | null;
  method?: "GET" | "HEAD";
  status_code?: number;
  content_type?: string;
  response_headers?: Record<string, string[]> | null;
  image_base64?: string | null;
}

function ProbeOutput({ result }: { result?: ProbeResult }) {
  const [imageError, setImageError] = useState(false);
  useEffect(() => setImageError(false), [result]);
  if (!result) return null;
  return (
    <div className="probe-result">
      <div className={`probe-summary ${result.ok ? "mint" : "probe-error"}`} role="status">
        <strong>{result.ok ? "执行完成" : "执行失败"}</strong>
        {result.status_code !== undefined && <span>HTTP {result.status_code}</span>}
        <span>总耗时 {result.elapsed_ms.toFixed(1)} ms</span>
        {result.exit_code !== null && <span>退出码 {result.exit_code}</span>}
      </div>
      <p className="probe-target">
        <code>{result.target}</code>
        {result.ip && <span> · {result.ip}</span>}
      </p>
      {result.method === "GET" && result.response_headers && (
        <details open>
          <summary>响应头</summary>
          <pre className="logs probe-output">
            {Object.entries(result.response_headers)
              .flatMap(([name, values]) => values.map((value) => `${name}: ${value}`))
              .join("\n") || "（未收到响应头）"}
          </pre>
        </details>
      )}
      {result.image_base64 ? (
        <figure className="probe-image">
          {imageError ? (
            <p className="probe-error">浏览器无法预览此图片，请结合响应头和诊断日志分析。</p>
          ) : (
            <img
              src={`data:${result.content_type?.split(";")[0]};base64,${result.image_base64}`}
              alt={`来自 ${result.target} 的图片响应`}
              onError={() => setImageError(true)}
            />
          )}
          <figcaption>{result.content_type}</figcaption>
        </figure>
      ) : (
        <details open>
          <summary>
            {result.method === "HEAD" ? "响应头" : result.method ? "响应内容" : "命令输出"}
          </summary>
          <pre className="logs probe-output">{result.stdout || "（无输出）"}</pre>
        </details>
      )}
      {result.stderr && (
        <details open>
          <summary>{result.method ? "请求、响应头与连接详情（-vvv）" : "诊断日志"}</summary>
          <pre className="logs probe-output">{result.stderr}</pre>
        </details>
      )}
    </div>
  );
}

export function NetworkProbes({ tool }: { tool: "tcping" | "curl" }) {
  const [tcpHost, setTcpHost] = useToolPreference<string>("tcping-host", "example.com");
  const [tcpPort, setTcpPort] = useToolPreference<number>("tcping-port", 443);
  const [url, setUrl] = useToolPreference<string>("curl-url", "https://example.com");
  const [method, setMethod] = useToolPreference<"GET" | "HEAD">("curl-method", "GET", [
    "GET",
    "HEAD",
  ]);
  const [tcpResult, setTcpResult] = useState<ProbeResult>();
  const [curlResult, setCurlResult] = useState<ProbeResult>();
  const [pending, setPending] = useState<string>();
  const [error, setError] = useState<{ tool: string; message: string }>();

  async function run(
    probeTool: string,
    input: object,
    setResult: (value: ProbeResult | undefined) => void,
  ) {
    if (pending) return;
    setPending(probeTool);
    setError(undefined);
    setResult(undefined);
    try {
      setResult(
        await request<ProbeResult>(`/tools/${probeTool}`, {
          method: "POST",
          body: JSON.stringify(input),
        }),
      );
    } catch (requestError) {
      setError({
        tool: probeTool,
        message: requestError instanceof Error ? requestError.message : "探测失败",
      });
    } finally {
      setPending(undefined);
    }
  }

  return (
    <>
      {tool === "tcping" && (
        <section className="panel">
          <div className="panel-head">
            <div>
              <h2>TCPing</h2>
              <p>建立 TCP 连接，查看端口可达性与总耗时。</p>
            </div>
            <Network size={20} />
          </div>
          <form
            className="panel-body probe-form"
            onSubmit={(event) => {
              event.preventDefault();
              void run("tcping", { host: tcpHost, port: tcpPort }, setTcpResult);
            }}
          >
            <label className="field">
              <span>TCPing 目标</span>
              <input
                required
                value={tcpHost}
                onChange={(event) => setTcpHost(event.target.value)}
                placeholder="域名或 IPv4 地址"
              />
            </label>
            <label className="field probe-port">
              <span>目标端口</span>
              <input
                required
                type="number"
                min={1}
                max={65535}
                value={tcpPort}
                onChange={(event) => setTcpPort(Number(event.target.value))}
              />
            </label>
            <button className="button primary" disabled={!!pending}>
              {pending === "tcping" ? "探测中…" : "开始 TCPing"}
            </button>
          </form>
          {error?.tool === "tcping" && (
            <p className="probe-error probe-error-message" role="alert">
              {error.message}
            </p>
          )}
          <ProbeOutput result={tcpResult} />
        </section>
      )}
      {tool === "curl" && (
        <section className="panel">
          <div className="panel-head">
            <div>
              <h2>CURL</h2>
              <p>
                GET / HEAD 探测，使用 -vvv 记录连接与响应详情。最多读取 2 MiB，单次请求最长 20 秒。
              </p>
            </div>
            <Globe2 size={20} />
          </div>
          <form
            className="panel-body probe-form curl-form"
            onSubmit={(event) => {
              event.preventDefault();
              void run("curl", { url, method }, setCurlResult);
            }}
          >
            <label className="field probe-method">
              <span>请求方法</span>
              <select
                value={method}
                onChange={(event) => setMethod(event.target.value as "GET" | "HEAD")}
              >
                <option value="GET">GET</option>
                <option value="HEAD">HEAD</option>
              </select>
            </label>
            <label className="field">
              <span>请求地址</span>
              <input
                required
                type="url"
                value={url}
                onChange={(event) => setUrl(event.target.value)}
                placeholder="https://example.com"
              />
            </label>
            <button className="button primary" disabled={!!pending}>
              {pending === "curl" ? "请求中…" : "执行 CURL"}
            </button>
          </form>
          {error?.tool === "curl" && (
            <p className="probe-error probe-error-message" role="alert">
              {error.message}
            </p>
          )}
          <ProbeOutput result={curlResult} />
        </section>
      )}
      <div className="note">
        <Network size={18} />
        <p>
          探测从容器发起，遵循当前网络路由和出口保护规则。域名使用已配置的 HTTPDNS
          解析，超时或拒绝连接会保留诊断结果。
        </p>
      </div>
    </>
  );
}
