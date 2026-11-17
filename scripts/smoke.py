#!/usr/bin/env python3
"""开发/CI 专用：隔离网络验证真实 VPN；此脚本和测试夹具均不进入产品镜像。"""

import argparse
import base64
import copy
import json
from pathlib import Path
import secrets
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid


def run(*args, check=True, input=None):
    result = subprocess.run(args, input=input, text=True, capture_output=True)
    if check and result.returncode:
        raise RuntimeError(f"命令失败 {args[:3]}：{result.stderr or result.stdout}")
    return result


def wait(work, timeout=60):
    deadline, last = time.monotonic() + timeout, None
    while time.monotonic() < deadline:
        try:
            result = work()
            if result:
                return result
        except (OSError, RuntimeError, urllib.error.URLError, TimeoutError) as error:
            last = error
        time.sleep(0.5)
    raise RuntimeError(f"等待超时：{last}")


def main(image):
    identifier = "vpnweb-test-" + secrets.token_hex(4)
    server, client, network = identifier + "-server", identifier + "-client", identifier + "-net"
    api_port = 8080
    authorization = "Basic " + base64.b64encode(b"admin:test-admin-password").decode()

    def host_port(port):
        return run("docker", "port", client, f"{port}/tcp").stdout.strip().rsplit(":", 1)[1]

    def api(path, method="GET", data=None, authorized=True, origin=None):
        headers = {"Content-Type": "application/json"}
        if authorized:
            headers["Authorization"] = authorization
        if origin or method != "GET":
            headers["Origin"] = origin or f"http://127.0.0.1:{host_port(api_port)}"
        request = urllib.request.Request(f"http://127.0.0.1:{host_port(api_port)}/api{path}", data=json.dumps(data).encode() if data is not None else None, headers=headers, method=method)
        try:
            with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=180) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            if error.code not in [401, 403]:
                raise RuntimeError(error.read().decode()) from error
            raise

    def healthz():
        request = urllib.request.Request(f"http://127.0.0.1:{host_port(api_port)}/healthz")
        try:
            with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=5) as response:
                return response.status
        except urllib.error.HTTPError as error:
            return error.code

    def container(*args, check=True):
        return run("docker", "exec", client, *args, check=check)

    def proxy(port, kind, auth=None, success=True):
        args = ["curl", "--silent", "--show-error", "--fail", "--max-time", "5", "--noproxy", "", "--proxy", f"{kind}://127.0.0.1:{host_port(port)}"]
        if auth:
            args += ["--proxy-user", auth]
        result = run(*args, "http://target.test:18080/", check=False)
        if success and (result.returncode or "vpnweb-test-ok" not in result.stdout):
            raise RuntimeError("代理请求失败：" + result.stderr)
        if not success and result.returncode == 0:
            raise RuntimeError("未认证或断线请求不应成功")
        return True

    def verify_blocked(address):
        result = container("curl", "--noproxy", "*", "--max-time", "2", f"http://{address}:18081/", check=False)
        assert result.returncode != 0, "出口保护失败：允许了隧道外直接连接"

    def ssh(directory, port=2222, command="printf ssh-ok", password=False):
        args = ["ssh", "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null", "-p", host_port(port)]
        if password:
            askpass = directory / "askpass"
            askpass.write_text("#!/bin/sh\necho ssh-test-password\n")
            askpass.chmod(0o700)
            args = ["env", f"SSH_ASKPASS={askpass}", "SSH_ASKPASS_REQUIRE=force", "DISPLAY=:0", *args, "-o", "PubkeyAuthentication=no", "-o", "NumberOfPasswordPrompts=1"]
        else:
            args += ["-i", str(directory / "ssh_key"), "-o", "BatchMode=yes"]
        return run(*args, "work@127.0.0.1", command).stdout

    try:
        run("docker", "network", "create", network)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            # 夹具通过只读挂载注入，产品镜像不包含任何测试程序。
            for name in ["ca", "server", "client"]:
                run("openssl", "req", "-new", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:P-256", "-nodes", "-keyout", str(directory / f"{name}.key"), "-out", str(directory / f"{name}.csr"), "-subj", f"/CN={name}")
            run("openssl", "x509", "-req", "-in", str(directory / "ca.csr"), "-signkey", str(directory / "ca.key"), "-out", str(directory / "ca.crt"), "-days", "1", "-extfile", "/dev/stdin", input="basicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign\n")
            for name in ["server", "client"]:
                run("openssl", "x509", "-req", "-in", str(directory / f"{name}.csr"), "-CA", str(directory / "ca.crt"), "-CAkey", str(directory / "ca.key"), "-CAcreateserial", "-out", str(directory / f"{name}.crt"), "-days", "1", "-extfile", "/dev/stdin", input=f"extendedKeyUsage={'serverAuth' if name == 'server' else 'clientAuth'}\nkeyUsage=digitalSignature\n")
            run("ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(directory / "ssh_key"))
            (directory / "server.conf").write_text('''server 10.77.0.0 255.255.255.0
dev tun0
proto udp4
port 1194
topology subnet
ca /test/ca.crt
cert /test/server.crt
key /test/server.key
dh none
push "dhcp-option DNS 10.77.0.1"
keepalive 2 10
verb 3
''')
            (directory / "fixture.py").write_text('''import http.server, json, socket, struct, sys, threading, urllib.parse
address = sys.argv[1]
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        query = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
        name = query.get("name", [""])[0]
        body = json.dumps({"Status": 0, "Answer": [{"type": 1, "data": address if name == "vpn.test" else "10.77.0.1"}]}).encode() if name else b"vpnweb-test-ok"
        self.send_response(200); self.end_headers(); self.wfile.write(body)
    def do_HEAD(self):
        self.send_response(200); self.send_header("Content-Length", "1024"); self.end_headers()
    def log_message(self, *args): pass
def dns():
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); sock.bind(("0.0.0.0", 53))
    while True:
        data, peer = sock.recvfrom(4096)
        position = 12
        while data[position]: position += data[position] + 1
        end = position + 5
        answer = data[:2]+struct.pack("!HHHHH", 0x8180, 1, 1, 0, 0)+data[12:end]
        answer += b"\\xc0\\x0c"+struct.pack("!HHIH", 1, 1, 30, 4)+socket.inet_aton("10.77.0.1")
        sock.sendto(answer, peer)
threading.Thread(target=dns, daemon=True).start()
threading.Thread(target=http.server.ThreadingHTTPServer(("0.0.0.0", 18081), Handler).serve_forever, daemon=True).start()
http.server.ThreadingHTTPServer(("0.0.0.0", 18080), Handler).serve_forever()
''')
            run("docker", "run", "-d", "--name", server, "--network", network, "--cap-add", "NET_ADMIN", "--device", "/dev/net/tun", "-v", f"{directory}:/test:ro", "--entrypoint", "/usr/sbin/openvpn", image, "--config", "/test/server.conf")
            address = run("docker", "inspect", "--format", f'{{{{(index .NetworkSettings.Networks "{network}").IPAddress}}}}', server).stdout.strip()
            run("docker", "exec", "-d", server, "python3", "/test/fixture.py", address)
            args = ["docker", "run", "-d", "--name", client, "--network", network, "--cap-add", "NET_ADMIN", "--device", "/dev/net/tun", "--sysctl", "net.ipv4.conf.all.src_valid_mark=1", "--sysctl", "net.ipv4.conf.all.rp_filter=2", "--tmpfs", "/run", "-e", "ADMIN_USERNAME=admin", "-e", "ADMIN_PASSWORD=test-admin-password"]
            for port in [8080, 8081, 8888, 8889, 1080, 1081, 2222, 2223]:
                args += ["-p", f"127.0.0.1::{port}"]
            run(*args, image)
            document = wait(lambda: api("/config"))
            wait(lambda: any(item["name"] == "http" and item["state"] == "RUNNING" for item in api("/status")["services"]))
            try:
                api("/config", authorized=False)
                raise RuntimeError("配置接口未要求 Basic 认证")
            except urllib.error.HTTPError as error:
                assert error.code == 401
            try:
                api("/profiles", "POST", {"name": "不应创建"}, origin="https://external.invalid")
                raise RuntimeError("跨站配置修改未被拒绝")
            except urllib.error.HTTPError as error:
                assert error.code == 403
            verify_blocked(address)
            wait(lambda: healthz() == 200)
            initial = api("/status")["health"]
            assert initial["container"]["healthy"] and initial["vpn"]["state"] == "unconfigured"
            defaults = document["profiles"][0]["settings"]["health"]
            assert defaults["action"] == "restart" and defaults["failure_threshold"] == 3
            container("supervisorctl", "-c", "/etc/supervisor/supervisord.conf", "stop", "health")
            assert healthz() == 503, "健康检查任务停止后容器仍健康"
            container("supervisorctl", "-c", "/etc/supervisor/supervisord.conf", "start", "health")
            wait(lambda: healthz() == 200)
            print("✓ Basic 认证、默认服务、连接前出口保护、容器与 VPN 健康分离", flush=True)

            settings = document["profiles"][0]["settings"]
            endpoint = str(uuid.uuid4())
            settings["vpn"].update({"config": "client\ndev tun\nremote-cert-tls server\nresolv-retry infinite\n", "endpoints": [{"id": str(uuid.uuid4()), "name": "坏端口", "host": "vpn.test", "port": 1195, "protocol": "udp"}, {"id": endpoint, "name": "可用 UDP", "host": "vpn.test", "port": 1194, "protocol": "udp"}], "selected_endpoint": endpoint, "dns_mode": "pushed", "httpdns_url": "http://httpdns.test:18080/resolve", "httpdns_ips": [address], "httpdns_protocol": "json", "ca": (directory / "ca.crt").read_text(), "cert": (directory / "client.crt").read_text(), "key": (directory / "client.key").read_text()})
            settings["socks5"]["enabled"] = True
            settings["sshd"].update({"enabled": True, "authorized_keys": (directory / "ssh_key.pub").read_text()})
            api("/config", "PUT", document)
            api("/apply", "POST")
            state = wait(lambda: (state if (state := api("/status"))["vpn"]["state"] == "CONNECTED" else None))
            assert state["selected_endpoint"]["endpoint_id"] == endpoint
            assert len([item for item in state["endpoint_probes"] if item["ok"]]) == 1
            assert state["dns"]["pushed"] == ["10.77.0.1"]
            wait(lambda: proxy(1080, "socks5h"))
            proxy(8888, "http")
            assert ssh(directory, command="test ! -r /config/config.yaml && test ! -r /config/secrets && test ! -w /config && printf ssh-ok") == "ssh-ok"
            saved_yaml = container("cat", "/config/config.yaml").stdout
            assert "-----BEGIN" not in saved_yaml and "test-admin-password" not in saved_yaml
            assert container("stat", "-c", "%a", "/config/config.yaml", "/config/secrets").stdout.split() == ["600", "700"]
            secret_modes = container("sh", "-c", "stat -c '%a %U' /config/secrets/*").stdout.splitlines()
            assert secret_modes and all(mode == "600 root" for mode in secret_modes)
            print("✓ 域名 HTTPDNS、真实 UDP 自动选点、下发 DNS、双代理、公钥 SSH、密钥权限", flush=True)

            pid = next(item["pid"] for item in state["services"] if item["name"] == "openvpn")
            settings["vpn"].update({"dns_mode": "manual", "dns": ["10.77.0.1"]})
            api("/config", "PUT", document)
            api("/dns", "POST")
            state = api("/status")
            assert state["dns"]["source"] == "manual"
            assert next(item["pid"] for item in state["services"] if item["name"] == "openvpn") == pid
            results = api("/tools/dns", "POST", {"domain": "target.test"})
            assert any(item["ok"] and item["ip"] == address for item in results["httpdns"])
            assert any(item["ok"] for item in results["vpn_dns"])
            for kind in ["ping", "tcp", "http"]:
                settings["health"].update({"enabled": True, "kind": kind, "target": "10.77.0.1", "port": 18080, "url": "http://10.77.0.1:18080/", "interval_secs": 5, "action": "none"})
                api("/config", "PUT", document)
                result = api("/health", "POST")
                assert result["ok"], result
            settings["health"]["http_method"] = "HEAD"
            api("/config", "PUT", document)
            assert api("/health", "POST")["ok"], "HEAD 健康检查失败"
            settings["health"].update({"http_method": "GET", "expected_status": 503})
            api("/config", "PUT", document)
            container("supervisorctl", "-c", "/etc/supervisor/supervisord.conf", "stop", "health")
            previous_health = api("/status")["health"]["vpn"]
            assert not api("/health", "POST")["ok"], "错误状态码未标记失败"
            after_test = api("/status")["health"]["vpn"]
            assert (after_test["state"], after_test["consecutive_failures"]) == (previous_health["state"], previous_health["consecutive_failures"]), "手动测试改变了周期健康状态"
            container("supervisorctl", "-c", "/etc/supervisor/supervisord.conf", "start", "health")
            settings["health"]["expected_status"] = 200
            api("/config", "PUT", document)
            assert api("/health", "POST")["ok"]
            print("✓ DNS 热更新不重启 VPN、全部 DNS 工具、Ping/TCP/HTTP 与 HEAD 健康检查", flush=True)

            # 真实故障验证：默认三次失败重启 VPN，容器持续健康，其他服务保持运行。
            before = api("/status")
            old_pid = next(item["pid"] for item in before["services"] if item["name"] == "openvpn")
            other_pids = {item["name"]: item["pid"] for item in before["services"] if item["name"] in ["backend", "http", "sshd"]}
            settings["health"].update({"expected_status": 503, "interval_secs": 1, "timeout_secs": 1, "failure_threshold": 3, "action": "restart", "action_cooldown_secs": 60})
            api("/config", "PUT", document)
            recovered = wait(lambda: (health if (health := api("/status")["health"])["vpn"]["last_action"] and health["vpn"]["last_action"]["state"] == "success" else None))
            assert recovered["container"]["healthy"] and healthz() == 200
            restarted = api("/status")
            assert next(item["pid"] for item in restarted["services"] if item["name"] == "openvpn") != old_pid
            assert {item["name"]: item["pid"] for item in restarted["services"] if item["name"] in other_pids} == other_pids
            attempt = recovered["vpn"]["last_action"]["attempted_at"]
            wait(lambda: api("/status")["health"]["vpn"]["consecutive_failures"] >= 6)
            assert api("/status")["health"]["vpn"]["last_action"]["attempted_at"] == attempt, "冷却期间重复执行动作"
            api("/services/openvpn/stop", "POST")
            time.sleep(4)
            stopped = api("/status")["health"]
            assert stopped["vpn"]["state"] == "stopped" and stopped["container"]["healthy"]
            assert stopped["vpn"]["last_action"]["attempted_at"] == attempt, "手动停止后触发了恢复"
            settings["health"].update({"expected_status": 200, "action": "none"})
            api("/config", "PUT", document)
            api("/services/openvpn/start", "POST")
            wait(lambda: api("/status")["vpn"]["state"] == "CONNECTED")
            for action in ["reconnect", "reapply"]:
                settings["health"].update({"expected_status": 503, "failure_threshold": 1, "action": action, "action_cooldown_secs": 4})
                api("/config", "PUT", document)
                wait(lambda: (last if (last := api("/status")["health"]["vpn"]["last_action"]) and last["action"] == action and last["state"] == "success" else None), timeout=90)
                settings["health"].update({"expected_status": 200, "action": "none"})
                api("/config", "PUT", document)
                wait(lambda: api("/status")["vpn"]["state"] == "CONNECTED")
            settings["health"].update({"expected_status": 503, "action": "none"})
            api("/config", "PUT", document)
            wait(lambda: api("/status")["health"]["vpn"]["state"] == "unhealthy")
            assert healthz() == 200, "VPN 网络异常影响了容器健康"
            settings["health"].update({"expected_status": 200, "failure_threshold": 3, "action": "restart", "action_cooldown_secs": 120, "interval_secs": 5, "timeout_secs": 5})
            api("/config", "PUT", document)
            wait(lambda: api("/status")["health"]["vpn"]["healthy"])
            print("✓ 默认三次失败恢复、可调阈值、冷却、软重连、重新应用配置、手动停止保护", flush=True)

            settings["http"].update({"authentication": "password", "username": "httpuser", "password": "http-password"})
            settings["socks5"].update({"authentication": "password", "username": "sockuser", "password": "socks-password"})
            settings["sshd"]["password"] = "ssh-test-password"
            api("/config", "PUT", document)
            for service in ["http", "socks5", "sshd"]:
                api(f"/services/{service}/restart", "POST")
            proxy(8888, "http", success=False)
            proxy(8888, "http", "httpuser:http-password")
            proxy(1080, "socks5h", success=False)
            proxy(1080, "socks5h", "sockuser:wrong", success=False)
            proxy(1080, "socks5h", "sockuser:socks-password")
            assert ssh(directory, password=True, command="printf password-ok") == "password-ok"
            print("✓ HTTP/SOCKS5 用户名密码认证、SSH 密码登录、独立服务重启", flush=True)

            # 新配置复制同一测试 VPN，但端口与认证完全独立。
            created = api("/profiles", "POST", {"name": "独立测试配置"})
            second_id = created["id"]
            document = created["config"]
            second = next(item for item in document["profiles"] if item["id"] == second_id)
            second["settings"] = copy.deepcopy(settings)
            second_settings = second["settings"]
            for service in ["http", "socks5", "sshd"]:
                second_settings[service]["port"] += 1
            second_settings["http"].update({"authentication": "none", "username": "", "password": ""})
            second_settings["vpn"]["selection_mode"] = "manual"
            second_settings["vpn"]["selected_endpoint"] = endpoint
            api("/config", "PUT", document)
            session = subprocess.Popen(["ssh", "-i", str(directory / "ssh_key"), "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null", "-o", "BatchMode=yes", "-p", host_port(2222), "work@127.0.0.1", "echo session-ready; exec sleep 600"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
            assert session.stdout.readline().strip() == "session-ready"
            api(f"/profiles/{second_id}/activate", "POST")
            assert session.wait(timeout=10) != 0, "切换配置后旧 SSH 会话仍在运行"
            wait(lambda: api("/status")["vpn"]["state"] == "CONNECTED")
            proxy(8889, "http")
            wait(lambda: proxy(1081, "socks5h", "sockuser:socks-password"))
            state = api("/status")
            assert state["active_profile"] == second_id
            assert not any(item["listening"] and item["port"] in [8888, 1080, 2222] for item in state["ports"])
            assert ssh(directory, 2223) == "ssh-ok"
            document = api("/config")
            assert document["profiles"][0]["settings"]["http"]["port"] == 8888
            document["web"]["port"] = 8081
            api("/config", "PUT", document)
            api("/services/backend/restart", "POST")
            api_port = 8081
            wait(lambda: api("/status")["web_port"] == 8081)
            api("/services/openvpn/stop", "POST")
            verify_blocked(address)
            proxy(8889, "http", success=False)
            proxy(1081, "socks5h", "sockuser:socks-password", success=False)
            assert ssh(directory, 2223, command="printf management-ok") == "management-ok"
            print("✓ 多配置端口/认证隔离、端口修改、管理服务重启、断线保护及 SSH 回程", flush=True)

            run("docker", "restart", client)
            wait(lambda: api("/status")["vpn"]["state"] == "CONNECTED")
            wait(lambda: proxy(1081, "socks5h", "sockuser:socks-password"))
            assert api("/config")["active_profile"] == second_id
            print("✓ 容器重启后 YAML 配置、密钥、认证与自动重连", flush=True)
    except Exception:
        for name in [client, server]:
            result = run("docker", "logs", "--tail", "60", name, check=False)
            print(result.stdout + result.stderr)
        print(run("docker", "exec", client, "sh", "-c", "tail -n 50 /config/logs/*.log /run/vpnweb/probe-*.log", check=False).stdout)
        raise
    finally:
        run("docker", "rm", "-f", "-v", client, server, check=False)
        run("docker", "network", "rm", network, check=False)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--image", default="openvpn-web:local")
    main(parser.parse_args().image)
