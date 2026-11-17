import { useEffect, useRef, useState } from "react";
import { Network, Pause, Play, Square } from "lucide-react";

type TraceTool = "mtr" | "traceroute";
interface Snapshot {
  tool: TraceTool;
  target: string;
  ip: string | null;
  elapsed_ms: number;
  stdout: string;
  stderr: string;
  done: boolean;
  ok: boolean;
  timed_out: boolean;
}

function MtrTable({ output }: { output: string }) {
  const hops = new Map<
    number,
    {
      ip: string;
      sent: number;
      received: number;
      total: number;
      last: number;
      best: number;
      worst: number;
    }
  >();
  for (const line of output.split("\n")) {
    const [event, index, value] = line.split(" ");
    if (!["h", "x", "p"].includes(event)) continue;
    const hop = Number(index);
    const row = hops.get(hop) ?? {
      ip: "*",
      sent: 0,
      received: 0,
      total: 0,
      last: 0,
      best: Infinity,
      worst: 0,
    };
    if (event === "h") row.ip = value;
    if (event === "x") row.sent += 1;
    if (event === "p") {
      const rtt = Number(value) / 1000;
      row.received += 1;
      row.total += rtt;
      row.last = rtt;
      row.best = Math.min(row.best, rtt);
      row.worst = Math.max(row.worst, rtt);
    }
    hops.set(hop, row);
  }
  if (!hops.size) return <p className="muted">等待路由探测结果…</p>;
  return (
    <div className="table-scroll">
      <table className="mtr-table">
        <thead>
          <tr>
            <th>跳数</th>
            <th>IP 地址</th>
            <th>已发送</th>
            <th>已收到</th>
            <th>未回包</th>
            <th>最近 ms</th>
            <th>平均 ms</th>
            <th>最小 ms</th>
            <th>最大 ms</th>
          </tr>
        </thead>
        <tbody>
          {[...hops]
            .sort(([a], [b]) => a - b)
            .map(([hop, row]) => (
              <tr key={hop}>
                <td>{hop + 1}</td>
                <td>
                  <code>{row.ip}</code>
                </td>
                <td>{row.sent}</td>
                <td>{row.received}</td>
                <td>
                  {row.sent
                    ? `${((Math.max(0, row.sent - row.received) / row.sent) * 100).toFixed(1)}%`
                    : "—"}
                </td>
                {[row.last, row.total / row.received, row.best, row.worst].map(
                  (rtt, index) => (
                    <td key={index}>{row.received ? rtt.toFixed(2) : "—"}</td>
                  ),
                )}
              </tr>
            ))}
        </tbody>
      </table>
    </div>
  );
}

export function TraceProbe({ tool }: { tool: TraceTool }) {
  const name = tool === "mtr" ? "MTR" : "Traceroute";
  const [host, setHost] = useState("1.1.1.1");
  const [phase, setPhase] = useState<
    "idle" | "connecting" | "running" | "done" | "stopped" | "error"
  >("idle");
  const [paused, setPaused] = useState(false);
  const [snapshot, setSnapshot] = useState<Snapshot>();
  const [error, setError] = useState("");
  const socketRef = useRef<WebSocket | null>(null);
  const latestRef = useRef<Snapshot | undefined>(undefined);
  const pausedRef = useRef(false);
  const finishedRef = useRef(false);
  const active = phase === "connecting" || phase === "running";

  useEffect(
    () => () => {
      const socket = socketRef.current;
      socketRef.current = null;
      socket?.close();
    },
    [],
  );

  function start() {
    if (socketRef.current) return;
    setSnapshot(undefined);
    latestRef.current = undefined;
    pausedRef.current = false;
    finishedRef.current = false;
    setPaused(false);
    setError("");
    setPhase("connecting");
    const url = new URL("/api/tools/trace", window.location.href);
    url.protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    url.search = new URLSearchParams({ tool, host: host.trim() }).toString();
    const socket = new WebSocket(url);
    socketRef.current = socket;
    socket.onopen = () => {
      if (socketRef.current === socket) setPhase("running");
    };
    socket.onmessage = (event) => {
      if (socketRef.current !== socket) return;
      const next = JSON.parse(event.data) as Snapshot;
      latestRef.current = next;
      if (!pausedRef.current) setSnapshot(next);
      if (next.done) {
        finishedRef.current = true;
        setPhase("done");
      }
    };
    socket.onerror = () => {
      if (socketRef.current === socket) {
        setError(
          "探测连接失败，请检查目标地址、管理认证和后端 WebSocket 代理。",
        );
      }
    };
    socket.onclose = () => {
      if (socketRef.current !== socket) return;
      socketRef.current = null;
      if (finishedRef.current) {
        setPhase("done");
      } else {
        setPhase("error");
        setError((current) => current || "探测连接意外断开，可重新开始。");
      }
    };
  }

  function togglePause() {
    const next = !pausedRef.current;
    pausedRef.current = next;
    setPaused(next);
    if (!next) setSnapshot(latestRef.current);
  }

  function terminate() {
    const socket = socketRef.current;
    socketRef.current = null;
    socket?.close();
    pausedRef.current = false;
    setPaused(false);
    setSnapshot(latestRef.current);
    setPhase("stopped");
  }

  const state =
    phase === "connecting"
      ? "正在连接…"
      : phase === "running"
        ? "探测中"
        : phase === "stopped"
          ? "已终止"
          : phase === "error"
            ? "连接失败"
            : phase === "done"
              ? paused
                ? "后端已结束"
                : snapshot?.timed_out
                  ? "已达 60 秒上限"
                  : snapshot?.ok
                    ? "执行完成"
                    : "执行结束"
              : "等待开始";
  return (
    <section className="panel">
      <div className="panel-head">
        <div>
          <h2>{name}</h2>
          <p>
            服务端每 2 秒推送结果，最多运行 60 秒；仅显示 IP，不做域名反解析。
          </p>
        </div>
        <Network size={20} />
      </div>
      <form
        className="panel-body probe-form"
        onSubmit={(event) => {
          event.preventDefault();
          start();
        }}
      >
        <label className="field">
          <span>{name} 目标</span>
          <input
            required
            value={host}
            disabled={active}
            onChange={(event) => setHost(event.target.value)}
            placeholder="域名或 IPv4 地址"
          />
        </label>
        <button className="button primary" disabled={active}>
          <Play size={16} />
          开始 {name}
        </button>
        <button
          type="button"
          className="button"
          disabled={!active && !paused}
          onClick={togglePause}
        >
          {paused ? <Play size={16} /> : <Pause size={16} />}
          {paused ? "恢复展示" : "暂停展示"}
        </button>
        <button
          type="button"
          className="button danger"
          disabled={!active}
          onClick={terminate}
        >
          <Square size={16} />
          终止探测
        </button>
      </form>
      <div className="probe-result">
        <div className="probe-summary" role="status">
          <strong>{state}</strong>
          {snapshot && (
            <span>运行 {(snapshot.elapsed_ms / 1000).toFixed(1)} 秒</span>
          )}
          {paused && <span>展示已暂停</span>}
        </div>
        <p className="muted trace-help">
          {paused
            ? phase === "done"
              ? "后端已完成并关闭连接，恢复展示可查看最终结果。"
              : "前端展示已暂停，后端继续探测；恢复时显示最新结果。"
            : "暂停仅冻结展示；终止会同时停止前后端探测。离开此工具页也会终止探测。"}
        </p>
        {error && (
          <p className="probe-error" role="alert">
            {error}
          </p>
        )}
        {snapshot && (
          <>
            <p className="probe-target">
              <code>{snapshot.target}</code>
              {snapshot.ip && <span> · {snapshot.ip}</span>}
            </p>
            {tool === "mtr" && (
              <>
                <MtrTable output={snapshot.stdout} />
                <p className="muted trace-help">
                  未回包包含仍在等待的请求；中间节点可能限制 ICMP
                  回包，请结合目标节点判断。
                </p>
              </>
            )}
            <details open={tool === "traceroute"}>
              <summary>
                {tool === "mtr" ? "MTR 原始输出" : "路由追踪结果"}
              </summary>
              <pre className="logs probe-output">
                {snapshot.stdout || "等待探测结果…"}
              </pre>
            </details>
            {snapshot.stderr && (
              <details open>
                <summary>诊断日志</summary>
                <pre className="logs probe-output">{snapshot.stderr}</pre>
              </details>
            )}
          </>
        )}
      </div>
    </section>
  );
}
