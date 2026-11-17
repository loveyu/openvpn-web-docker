use super::*;
use config::{Health, HealthAction};

pub(super) async fn firewall_ready() -> Result<bool> {
    let response = network::output(
        "nft",
        &args(&["-j", "list", "table", "inet", "vpnweb"]),
        None,
        1,
    )
    .await?;
    if !response.status.success() {
        return Ok(false);
    }
    let value: Value = serde_json::from_slice(&response.stdout)?;
    let chains = value["nftables"].as_array().ok_or("防火墙状态格式无效")?;
    Ok(["input", "output", "forward"].iter().all(|name| {
        chains.iter().any(|entry| {
            let chain = &entry["chain"];
            chain["family"] == "inet"
                && chain["table"] == "vpnweb"
                && chain["name"] == *name
                && chain["hook"] == *name
                && chain["type"] == "filter"
                && chain["policy"] == "drop"
        })
    }))
}

pub(super) fn container_status(services: &[Value], firewall: bool) -> Value {
    let mut errors = Vec::new();
    for (name, label) in [("backend", "管理服务"), ("health", "VPN 健康检查任务")] {
        if !services
            .iter()
            .any(|item| item["name"] == name && item["state"] == "RUNNING")
        {
            errors.push(format!("{label}未运行"));
        }
    }
    if !firewall {
        errors.push("出口防火墙的默认拒绝策略未就绪".into());
    }
    json!({"healthy":errors.is_empty(),"errors":errors})
}

fn identity(config: &Active) -> Value {
    // 仅记录探测和恢复参数，不把 VPN 凭据复制到状态响应中。
    json!({"profile_id":config.id,"settings":config.settings.health})
}

async fn observe(rt: &Runtime, config: &Active, probe: bool) -> Result<Value> {
    let active = rt.active()?;
    let mut result = json!({
        "identity":identity(config),"ok":false,"healthy":null,"state":"checking",
        "errors":[],"probe":{"enabled":config.settings.health.enabled},
    });
    let configured =
        !active.settings.vpn.config.trim().is_empty() || !active.settings.vpn.endpoints.is_empty();
    if active.id != config.id {
        result["errors"] = json!(["等待当前 VPN 配置应用完成"]);
    } else if !configured {
        result["state"] = json!("unconfigured");
        result["ok"] = json!(true);
    } else {
        let services = processes().await?;
        let process = services.iter().find(|item| item["name"] == "openvpn");
        match process.and_then(|item| item["state"].as_str()) {
            Some("STOPPED") => {
                result["state"] = json!("stopped");
                result["ok"] = json!(true);
            }
            Some("RUNNING" | "STARTING") => {
                let connection = vpn_state(rt).await;
                result["connection_since"] = connection["since"].clone();
                result["connection_ip"] = connection["ip"].clone();
                if connection["state"] != "CONNECTED" {
                    result["state"] = json!("connecting");
                    result["errors"] = json!(["OpenVPN 隧道尚未连接"]);
                } else {
                    let response = network::output(
                        "ip",
                        &args(&["-j", "-4", "address", "show", "dev", "tun0"]),
                        None,
                        1,
                    )
                    .await?;
                    let addresses: Value = if response.status.success() {
                        serde_json::from_slice(&response.stdout)?
                    } else {
                        json!([])
                    };
                    let available = addresses[0]["addr_info"]
                        .as_array()
                        .is_some_and(|items| items.iter().any(|item| item["family"] == "inet"));
                    if !available {
                        result["state"] = json!("unhealthy");
                        result["errors"] = json!(["tun0 没有可用的 IPv4 地址"]);
                    } else {
                        result["state"] = json!("healthy");
                        result["ok"] = json!(true);
                        if probe && config.settings.health.enabled {
                            result["probe"] = network::custom_health(&config.settings.health).await;
                            result["ok"] = result["probe"]["ok"].clone();
                            if result["ok"] != true {
                                result["state"] = json!("unhealthy");
                                result["errors"] = json!(["VPN 隧道内网络检查失败"]);
                            }
                        }
                    }
                }
            }
            _ => {
                result["state"] = json!("unhealthy");
                result["errors"] = json!(["OpenVPN 服务未正常运行"]);
            }
        }
    }
    result["checked_at"] = json!(network::timestamp());
    Ok(result)
}

fn idle(value: &Value) -> bool {
    matches!(
        value["state"].as_str(),
        Some("unconfigured" | "stopped" | "checking")
    )
}

fn fresh(sample: &Value, config: &Active, now: u64) -> bool {
    sample["identity"] == identity(config)
        && sample["checked_at"].as_u64().is_some_and(|checked| {
            checked <= now
                && now - checked
                    <= config.settings.health.interval_secs
                        + config.settings.health.timeout_secs
                        + 5
        })
}

fn summarize(mut live: Value, cached: &Value, config: &Active, now: u64) -> Value {
    live["consecutive_failures"] = json!(0);
    if idle(&live) {
        return live;
    }
    let current = fresh(cached, config, now)
        && (live["state"] != "healthy"
            || (live["connection_since"] == cached["connection_since"]
                && live["connection_ip"] == cached["connection_ip"]));
    if current {
        live["consecutive_failures"] = cached["consecutive_failures"].clone();
    }
    if live["state"] == "healthy" && config.settings.health.enabled {
        if !current || cached["probe"]["ok"].is_null() {
            live["state"] = json!("checking");
            live["errors"] = json!(["等待当前配置和连接的有效网络检查结果"]);
            return live;
        }
        live["probe"] = cached["probe"].clone();
        live["healthy"] = json!(cached["healthy"] == true);
        if live["healthy"] != true {
            live["state"] = json!("unhealthy");
            live["errors"] = cached["errors"].clone();
        }
    } else {
        live["healthy"] = json!(live["ok"] == true);
        if current
            && cached["consecutive_failures"].as_u64().unwrap_or(0)
                >= u64::from(config.settings.health.failure_threshold)
        {
            live["state"] = json!("unhealthy");
        }
    }
    live
}

pub(super) async fn vpn_status(rt: &Runtime) -> Result<Value> {
    let config = rt.saved()?;
    let live = observe(rt, &config, false).await?;
    let cached = rt.read("health.json", json!({}))?;
    let mut result = summarize(live, &cached, &config, network::timestamp());
    let action = rt.read("health-action.json", Value::Null)?;
    result["last_action"] = if action["profile_id"] == config.id {
        action
    } else {
        Value::Null
    };
    // 检查标识只用于内部校验，页面只接收状态与诊断结果。
    result.as_object_mut().unwrap().remove("identity");
    Ok(result)
}

pub(super) async fn check(rt: &Runtime) -> Result<Value> {
    let config = rt.saved()?;
    let mut result = observe(rt, &config, true).await?;
    result.as_object_mut().unwrap().remove("identity");
    // 手动测试不写周期结果，不改变计数，也不执行恢复动作。
    Ok(result)
}

fn action_due(settings: &Health, failures: u32, last: &Value, profile: &str, now: u64) -> bool {
    settings.action != HealthAction::None
        && failures >= settings.failure_threshold
        && (last["profile_id"] != profile
            || last["attempted_at"]
                .as_u64()
                .is_none_or(|at| now.saturating_sub(at) >= settings.action_cooldown_secs))
}

async fn recover(rt: &Runtime, expected: &Active) -> Result<()> {
    // 与页面的服务操作共享锁，获得锁后再次确认，防止恢复已被手动停止的 VPN。
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(rt.run.join("apply.lock"))?;
    lock.lock()?;
    let current = rt.saved()?;
    if identity(&current) != identity(expected) || rt.active()?.id != current.id {
        return Ok(());
    }
    let services = processes().await?;
    if services
        .iter()
        .any(|item| item["name"] == "openvpn" && item["state"] == "STOPPED")
    {
        return Ok(());
    }
    let mut record = json!({
        "profile_id":current.id,"action":current.settings.health.action,
        "attempted_at":network::timestamp(),"state":"running",
    });
    // 执行前先落盘，进程中断或动作失败时仍遵守冷却时间。
    rt.json("health-action.json", &record)?;
    let outcome: Result<()> = match current.settings.health.action {
        HealthAction::None => Ok(()),
        HealthAction::Reconnect => network::command(
            "supervisorctl",
            &args(&[
                "-c",
                "/etc/supervisor/supervisord.conf",
                "signal",
                "USR1",
                "openvpn",
            ]),
            None,
        )
        .await
        .map(|_| ()),
        HealthAction::Restart => supervisor("restart", Some("openvpn")).await.map(|_| ()),
        HealthAction::Reapply => apply(rt, "openvpn", "restart").await.map(|_| ()),
    };
    record["state"] = json!(if outcome.is_ok() { "success" } else { "failed" });
    record["message"] = json!(
        outcome
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "恢复动作已执行，等待后续检查确认".into())
    );
    rt.json("health-action.json", &record)?;
    println!("{record}");
    Ok(())
}

pub(super) async fn worker(rt: &Runtime) -> Result<Value> {
    let mut previous = Value::Null;
    let mut failures = 0_u32;
    let mut action_failures = 0_u32;
    let mut next = tokio::time::Instant::now();
    loop {
        let config = rt.saved()?;
        let current = identity(&config);
        if current != previous {
            failures = 0;
            action_failures = 0;
            next = tokio::time::Instant::now();
            previous = current;
        }
        if tokio::time::Instant::now() >= next {
            let mut sample = observe(rt, &config, true).await?;
            if sample["ok"] == true || idle(&sample) {
                failures = 0;
                action_failures = 0;
            } else {
                failures = failures.saturating_add(1);
                action_failures = action_failures.saturating_add(1);
            }
            sample["consecutive_failures"] = json!(failures);
            sample["healthy"] = if idle(&sample) {
                Value::Null
            } else {
                json!(failures < config.settings.health.failure_threshold)
            };
            rt.json("health.json", &sample)?;
            println!("{sample}");
            let last = rt.read("health-action.json", Value::Null)?;
            if !idle(&sample)
                && action_due(
                    &config.settings.health,
                    action_failures,
                    &last,
                    &config.id,
                    network::timestamp(),
                )
            {
                recover(rt, &config).await?;
                action_failures = 0;
            }
            next = tokio::time::Instant::now()
                + Duration::from_secs(config.settings.health.interval_secs);
        }
        // 保存参数或切换配置后及时开始新一轮检查，不等待旧间隔结束。
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Active {
        Active {
            id: "test-profile".into(),
            name: "测试".into(),
            web: Web::default(),
            settings: Config::default(),
        }
    }

    fn connected() -> Value {
        json!({"state":"healthy","ok":true,"healthy":null,"errors":[],
            "connection_since":100,"connection_ip":"192.0.2.1","probe":{"enabled":true}})
    }

    #[test]
    fn container_health_is_independent_of_vpn_and_proxy_services() {
        let core = vec![
            json!({"name":"backend","state":"RUNNING"}),
            json!({"name":"health","state":"RUNNING"}),
        ];
        assert_eq!(container_status(&core, true)["healthy"], true);
        assert_eq!(container_status(&core, false)["healthy"], false);
        assert_eq!(container_status(&core[..1], true)["healthy"], false);
    }

    #[test]
    fn stale_or_changed_probe_results_are_not_healthy() {
        let mut config = profile();
        config.settings.health.enabled = true;
        let cached = json!({"identity":identity(&config),"checked_at":100,"healthy":true,
            "connection_since":100,"connection_ip":"192.0.2.1","probe":{"ok":true},"consecutive_failures":0});
        assert_eq!(
            summarize(connected(), &cached, &config, 110)["healthy"],
            true
        );
        assert_eq!(
            summarize(connected(), &cached, &config, 141)["state"],
            "checking"
        );
        let mut reconnected = connected();
        reconnected["connection_since"] = json!(105);
        assert_eq!(
            summarize(reconnected, &cached, &config, 110)["state"],
            "checking"
        );
        config.settings.health.expected_status = 204;
        assert_eq!(
            summarize(connected(), &cached, &config, 110)["state"],
            "checking"
        );
    }

    #[test]
    fn stopped_and_unconfigured_vpn_do_not_reuse_failures() {
        let config = profile();
        let cached = json!({"identity":identity(&config),"checked_at":100,"healthy":false,"consecutive_failures":3});
        for state in ["stopped", "unconfigured"] {
            let result = summarize(json!({"state":state,"healthy":null}), &cached, &config, 110);
            assert_eq!(result["healthy"], Value::Null);
            assert_eq!(result["consecutive_failures"], 0);
        }
    }

    #[test]
    fn recovery_respects_threshold_cooldown_and_record_only_mode() {
        let mut settings = Health::default();
        assert_eq!(settings.action, HealthAction::Restart);
        assert!(!action_due(&settings, 2, &Value::Null, "test", 100));
        assert!(action_due(&settings, 3, &Value::Null, "test", 100));
        let last = json!({"profile_id":"test","attempted_at":100});
        assert!(!action_due(&settings, 3, &last, "test", 219));
        assert!(action_due(&settings, 3, &last, "test", 220));
        settings.failure_threshold = 5;
        assert!(!action_due(&settings, 3, &Value::Null, "test", 220));
        settings.action = HealthAction::None;
        assert!(!action_due(&settings, 5, &Value::Null, "test", 220));
    }
}
