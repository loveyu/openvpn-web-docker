use super::address;
use crate::config::Vpn;
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use serde::{Deserialize, Serialize};
use std::{net::Ipv4Addr, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    time::{Instant, MissedTickBehavior, interval_at, timeout, timeout_at},
};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Ping,
    Mtr,
    Traceroute,
}

#[derive(Serialize)]
struct Snapshot {
    tool: Tool,
    target: String,
    ip: Option<Ipv4Addr>,
    elapsed_ms: u128,
    stdout: String,
    stderr: String,
    done: bool,
    ok: bool,
    timed_out: bool,
}

async fn send(socket: &mut WebSocket, snapshot: &mut Snapshot, start: Instant) -> bool {
    snapshot.elapsed_ms = start.elapsed().as_millis();
    matches!(
        timeout(
            Duration::from_secs(2),
            socket.send(Message::Text(
                serde_json::to_string(snapshot).unwrap().into()
            ))
        )
        .await,
        Ok(Ok(()))
    )
}

async fn finish(socket: &mut WebSocket, snapshot: &mut Snapshot, start: Instant) {
    snapshot.done = true;
    if send(socket, snapshot, start).await {
        let _ = timeout(
            Duration::from_secs(2),
            socket.send(Message::Close(Some(CloseFrame {
                code: 1000,
                reason: "探测已结束".into(),
            }))),
        )
        .await;
    }
}

fn disconnected(message: Option<Result<Message, axum::Error>>) -> bool {
    matches!(message, None | Some(Err(_)) | Some(Ok(Message::Close(_))))
}

fn append(output: &mut String, line: &str) {
    // 工具输出只用于诊断，限制累计大小，避免慢客户端占用过多内存。
    if output.len() < 128 * 1024 {
        output.push_str(line);
        output.push('\n');
    }
}

async fn terminate(child: &mut Child, group: u32) {
    // MTR 会启动 mtr-packet；终止整个独立进程组，避免断开后遗留探测进程。
    let _ = Command::new("kill")
        .args(["-KILL", "--", &format!("-{group}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
    let _ = child.kill().await;
}

pub async fn run(mut socket: WebSocket, vpn: Vpn, tool: Tool, target: String) {
    let start = Instant::now();
    let deadline = start + Duration::from_secs(60);
    let mut ticks = interval_at(start + Duration::from_secs(2), Duration::from_secs(2));
    ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut snapshot = Snapshot {
        tool,
        target,
        ip: None,
        elapsed_ms: 0,
        stdout: String::new(),
        stderr: String::new(),
        done: false,
        ok: false,
        timed_out: false,
    };
    let ip = {
        let target = snapshot.target.clone();
        let resolution = timeout_at(deadline, address(&vpn, &target));
        tokio::pin!(resolution);
        loop {
            tokio::select! {
                result = &mut resolution => break result,
                message = socket.recv() => {
                    if disconnected(message) { return; }
                }
                _ = ticks.tick() => {
                    // 解析期间也保持两秒的服务端推送节奏。
                    if !send(&mut socket, &mut snapshot, start).await { return; }
                }
            }
        }
    };
    let ip = match ip {
        Ok(Ok(ip)) => ip,
        error => {
            snapshot.stderr = match error {
                Ok(Err(error)) => error.to_string(),
                Err(_) => {
                    snapshot.timed_out = true;
                    "已达到 60 秒上限".into()
                }
                _ => unreachable!(),
            };
            finish(&mut socket, &mut snapshot, start).await;
            return;
        }
    };
    snapshot.ip = Some(ip);
    let (program, args): (&str, &[&str]) = match tool {
        Tool::Ping => ("ping", &["-n", "-O", "-i", "1", "-W", "2"]),
        Tool::Mtr => (
            "mtr",
            &[
                "--raw",
                "--no-dns",
                "--report-cycles",
                "60",
                "--interval",
                "1",
                "--max-ttl",
                "30",
                "--timeout",
                "1",
            ],
        ),
        Tool::Traceroute => ("traceroute", &["-n", "-m", "30", "-q", "1", "-w", "1"]),
    };
    let child = Command::new(program)
        .args(args)
        .arg(ip.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true)
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            snapshot.stderr = format!("无法启动 {program}：{error}");
            finish(&mut socket, &mut snapshot, start).await;
            return;
        }
    };
    let group = child.id().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr = BufReader::new(child.stderr.take().unwrap()).lines();
    let mut stdout_closed = false;
    let mut stderr_closed = false;
    let mut status = None;
    let mut wait_failed = false;
    loop {
        if (status.is_some() || wait_failed) && stdout_closed && stderr_closed {
            break;
        }
        tokio::select! {
            message = socket.recv() => {
                if disconnected(message) {
                    terminate(&mut child, group).await;
                    return;
                }
            }
            line = stdout.next_line(), if !stdout_closed => match line {
                Ok(Some(line)) => append(&mut snapshot.stdout, &line),
                Ok(None) => stdout_closed = true,
                Err(error) => {
                    stdout_closed = true;
                    append(&mut snapshot.stderr, &error.to_string());
                }
            },
            line = stderr.next_line(), if !stderr_closed => match line {
                Ok(Some(line)) => append(&mut snapshot.stderr, &line),
                Ok(None) => stderr_closed = true,
                Err(error) => {
                    stderr_closed = true;
                    append(&mut snapshot.stderr, &error.to_string());
                }
            },
            result = child.wait(), if status.is_none() && !wait_failed => match result {
                Ok(result) => status = Some(result),
                Err(error) => {
                    wait_failed = true;
                    append(&mut snapshot.stderr, &error.to_string());
                }
            },
            _ = tokio::time::sleep_until(deadline), if !snapshot.timed_out => {
                snapshot.timed_out = true;
                terminate(&mut child, group).await;
                append(&mut snapshot.stderr, "已达到 60 秒上限，探测已停止。");
            }
            _ = ticks.tick() => {
                if !send(&mut socket, &mut snapshot, start).await {
                    terminate(&mut child, group).await;
                    return;
                }
            }
        }
    }
    snapshot.ok = !snapshot.timed_out && status.is_some_and(|status| status.success());
    finish(&mut socket, &mut snapshot, start).await;
}
