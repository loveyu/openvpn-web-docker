import { execFileSync, spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { createServer } from "node:net";
import { setTimeout } from "node:timers/promises";

const session = "openvpn-web-dev";
const [action, frontendPort] = process.argv.slice(2);

function settings() {
  const config = JSON.parse(readFileSync(0, "utf8"));
  const names = new Map([
    [80, ["后端 API", "WEB_PORT"]],
    [8888, ["HTTP 代理", "HTTP_PROXY_PORT"]],
    [1080, ["SOCKS5", "SOCKS5_PORT"]],
    [22, ["SSH", "SSH_PORT"]],
    [9001, ["Supervisor", "SUPERVISOR_PORT"]],
  ]);
  const ports = config.services["openvpn-web"].ports.map((binding) => {
    const [name, variable] = names.get(binding.target);
    return {
      name,
      variable,
      host: binding.host_ip || "0.0.0.0",
      port: Number(binding.published),
      target: binding.target,
    };
  });
  ports.unshift({
    name: "前端",
    variable: "FRONTEND_PORT",
    host: "127.0.0.1",
    port: Number(frontendPort),
  });
  for (const binding of ports) {
    if (!Number.isInteger(binding.port) || binding.port < 1 || binding.port > 65535) {
      throw new Error(`${binding.variable} 必须是 1–65535 的端口号。`);
    }
  }
  return ports;
}

function url(binding, path = "/") {
  let host = ["0.0.0.0", "::"].includes(binding.host) ? "127.0.0.1" : binding.host;
  if (host.includes(":")) host = `[${host}]`;
  return `http://${host}:${binding.port}${path}`;
}

async function checkPorts(ports) {
  const containers = execFileSync("docker", ["ps", "--format", "{{json .}}"], {
    encoding: "utf8",
  })
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
  const issues = await Promise.all(
    ports.map(async (binding) => {
      // Docker 可能只使用 NAT 发布端口，宿主机上未必存在可检测的监听进程。
      const owners = containers.filter((container) =>
        [...container.Ports.matchAll(/(\[[^\]]+\]|[\d.]+):(\d+)->\d+\/tcp/g)].some(
          ([, host, port]) =>
            Number(port) === binding.port &&
            (["0.0.0.0", "::", "[::]"].includes(binding.host) ||
              ["0.0.0.0", "::", "[::]", binding.host].includes(host)),
        ),
      );
      if (owners.length)
        return { binding, reason: `Docker 容器：${owners.map((c) => c.Names).join("、")}` };
      return new Promise((resolve) => {
        const server = createServer();
        server.once("error", (error) =>
          resolve({
            binding,
            reason: error.code === "EADDRINUSE" ? "端口已被其他进程占用" : error.code,
          }),
        );
        server.listen({ host: binding.host, port: binding.port, exclusive: true }, () => {
          server.close(() => resolve(null));
        });
      });
    }),
  );
  const conflicts = issues.filter(Boolean);
  if (!conflicts.length) return;
  console.error("启动前检查失败，请先处理以下端口：");
  for (const { binding, reason } of conflicts) {
    console.error(`  ${binding.name} ${binding.host}:${binding.port}：${reason}`);
    let command;
    if (process.platform === "linux") {
      command = ["ss", ["-ltnp", `sport = :${binding.port}`]];
      console.error(`    查看占用：ss -ltnp 'sport = :${binding.port}'`);
    } else {
      command = ["lsof", ["-nP", `-iTCP:${binding.port}`, "-sTCP:LISTEN"]];
      console.error(`    查看占用：lsof -nP -iTCP:${binding.port} -sTCP:LISTEN`);
    }
    const detail = spawnSync(command[0], command[1], { encoding: "utf8" });
    if (detail.status === 0)
      console.error(
        detail.stdout
          .trim()
          .split("\n")
          .map((line) => `    ${line}`)
          .join("\n"),
      );
    console.error(`    调整端口：在 .env 中设置 ${binding.variable}`);
  }
  if (
    conflicts.some(
      ({ reason }) =>
        reason.startsWith("Docker 容器：") &&
        reason.slice("Docker 容器：".length).split("、").includes("openvpn-web"),
    )
  ) {
    console.error("  停止本项目原后端：docker compose stop openvpn-web");
  }
  throw new Error("开发环境尚未启动；端口处理后重新执行 ./scripts/dev.sh start。");
}

async function waitUntilReady(ports) {
  const frontend = ports[0];
  const backend = ports.find((binding) => binding.target === 80);
  const supervisor = ports.find((binding) => binding.target === 9001);
  const checks = [
    url(frontend),
    url(frontend, "/healthz"),
    url(backend, "/healthz"),
    url(supervisor),
  ];
  console.log(`正在等待前端和后端就绪；构建输出：tmux attach-session -t ${session}`);
  for (let attempt = 0; attempt < 600; attempt++) {
    for (const window of ["frontend", "backend"]) {
      const dead = execFileSync(
        "tmux",
        ["display-message", "-p", "-t", `=${session}:${window}`, "#{pane_dead}"],
        {
          encoding: "utf8",
        },
      ).trim();
      if (dead === "1") {
        throw new Error(
          `${window} 窗口已退出，请执行 tmux attach-session -t ${session} 查看错误。`,
        );
      }
    }
    const responses = await Promise.allSettled(
      checks.map(async (address) => {
        const response = await fetch(address, { signal: AbortSignal.timeout(2000) });
        await response.body?.cancel();
        return response.status;
      }),
    );
    if (
      responses.every(
        (result, index) =>
          result.status === "fulfilled" &&
          (result.value === 200 || (index === 3 && result.value === 401)),
      )
    ) {
      console.log("开发环境已就绪：");
      console.log(`  前端：${url(frontend)}`);
      console.log(`  后端健康检查：${url(backend, "/healthz")}`);
      console.log(`  Supervisor：${url(supervisor)}`);
      return;
    }
    if (attempt > 0 && attempt % 10 === 0) console.log("仍在构建或启动，等待服务就绪……");
    await setTimeout(1000);
  }
  throw new Error(`等待服务就绪超时，请执行 tmux attach-session -t ${session} 查看输出。`);
}

try {
  const ports = settings();
  if (action === "check") await checkPorts(ports);
  else if (action === "wait") await waitUntilReady(ports);
  else throw new Error(`未知开发检查操作：${action}`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
