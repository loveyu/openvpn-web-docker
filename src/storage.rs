use crate::config::{ConfigDocument, Vpn};
use std::{
    fs, io,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};
use uuid::Uuid;

pub fn private_write(path: &Path, content: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(content)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

fn materials(vpn: &mut Vpn) -> [&mut String; 5] {
    [
        &mut vpn.ca,
        &mut vpn.cert,
        &mut vpn.key,
        &mut vpn.tls_auth,
        &mut vpn.tls_crypt,
    ]
}

fn extract_inline(vpn: &mut Vpn) -> io::Result<()> {
    let text = vpn.config.clone();
    let mut lines = text.lines();
    let mut kept = Vec::new();
    while let Some(line) = lines.next() {
        let tag = line.trim();
        let destination = match tag {
            "<ca>" => Some(&mut vpn.ca),
            "<cert>" => Some(&mut vpn.cert),
            "<key>" => Some(&mut vpn.key),
            "<tls-auth>" => Some(&mut vpn.tls_auth),
            "<tls-crypt>" => Some(&mut vpn.tls_crypt),
            _ => None,
        };
        if let Some(destination) = destination {
            if !destination.is_empty() {
                return Err(io::Error::other("内联证书与表单内容不能重复"));
            }
            let ending = tag.replacen('<', "</", 1);
            let mut content = Vec::new();
            for value in lines.by_ref() {
                if value.trim() == ending {
                    break;
                }
                content.push(value);
            }
            *destination = content.join("\n") + "\n";
        } else {
            kept.push(line);
        }
    }
    if !vpn.tls_auth.is_empty() {
        for line in &kept {
            let words = line.split_whitespace().collect::<Vec<_>>();
            if words.first() == Some(&"key-direction") && words.len() == 2 {
                vpn.tls_auth_direction = words[1].parse().map_err(io::Error::other)?;
            }
        }
    }
    vpn.config = kept.join("\n");
    Ok(())
}

fn stored_document(text: &str) -> io::Result<ConfigDocument> {
    let mut value: serde_json::Value = serde_saphyr::from_str(text).map_err(io::Error::other)?;
    // 仅兼容磁盘上的旧端口字段；API 反序列化仍拒绝修改服务监听端口。
    if let Some(document) = value.as_object_mut() {
        document.remove("web");
    }
    if let Some(profiles) = value
        .get_mut("profiles")
        .and_then(|profiles| profiles.as_array_mut())
    {
        for profile in profiles {
            for service in ["http", "socks5", "sshd"] {
                if let Some(settings) = profile
                    .get_mut("settings")
                    .and_then(|settings| settings.get_mut(service))
                    .and_then(|settings| settings.as_object_mut())
                {
                    settings.remove("port");
                }
            }
        }
    }
    serde_json::from_value(value).map_err(io::Error::other)
}

pub fn load(directory: &Path) -> io::Result<ConfigDocument> {
    let mut document = stored_document(&fs::read_to_string(directory.join("config.yaml"))?)?;
    for profile in &mut document.profiles {
        for reference in materials(&mut profile.settings.vpn) {
            if !reference.is_empty() {
                Uuid::parse_str(reference).map_err(io::Error::other)?;
                *reference = fs::read_to_string(directory.join("secrets").join(&*reference))?;
            }
        }
    }
    Ok(document)
}

pub fn editor_load(directory: &Path) -> io::Result<ConfigDocument> {
    let mut document = load(directory)?;
    let last = fs::read_to_string(directory.join(".state/last-endpoints.yaml"))
        .ok()
        .and_then(|value| {
            serde_saphyr::from_str::<std::collections::HashMap<String, String>>(&value).ok()
        })
        .unwrap_or_default();
    for profile in &mut document.profiles {
        if let Some(endpoint) = last.get(&profile.id)
            && profile
                .settings
                .vpn
                .endpoints
                .iter()
                .any(|item| &item.id == endpoint)
        {
            profile.settings.vpn.selected_endpoint = endpoint.clone();
        }
    }
    Ok(document)
}

pub fn save(directory: &Path, document: &ConfigDocument) -> io::Result<()> {
    let secrets = directory.join("secrets");
    fs::create_dir_all(&secrets)?;
    fs::set_permissions(&secrets, fs::Permissions::from_mode(0o700))?;
    let old: Option<ConfigDocument> = fs::read_to_string(directory.join("config.yaml"))
        .ok()
        .and_then(|value| stored_document(&value).ok());
    let mut stored = document.clone();
    for profile in &mut stored.profiles {
        extract_inline(&mut profile.settings.vpn)?;
        let mut previous = old
            .as_ref()
            .and_then(|value| value.profiles.iter().find(|item| item.id == profile.id))
            .map(|value| value.settings.vpn.clone())
            .unwrap_or_default();
        for (value, old_id) in materials(&mut profile.settings.vpn)
            .into_iter()
            .zip(materials(&mut previous))
        {
            if value.is_empty() {
                continue;
            }
            if !old_id.is_empty()
                && Uuid::parse_str(old_id).is_ok()
                && fs::read_to_string(secrets.join(&*old_id)).ok().as_deref()
                    == Some(value.as_str())
            {
                *value = old_id.clone();
            } else {
                let id = Uuid::new_v4().to_string();
                private_write(&secrets.join(&id), value.as_bytes())?;
                *value = id;
            }
        }
    }
    let yaml = serde_saphyr::to_string(&stored).map_err(io::Error::other)?;
    private_write(&directory.join("config.yaml"), yaml.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loads_and_resaves_legacy_configs_without_service_port_fields() {
        let directory = tempfile::tempdir().unwrap();
        let document = ConfigDocument::default();
        let mut legacy = serde_json::to_value(&document).unwrap();
        legacy["web"] = serde_json::json!({"port": 8080});
        for service in ["http", "socks5", "sshd"] {
            legacy["profiles"][0]["settings"][service]["port"] = 2222.into();
        }
        let path = directory.path().join("config.yaml");
        fs::write(&path, serde_saphyr::to_string(&legacy).unwrap()).unwrap();

        let loaded = load(directory.path()).unwrap();
        assert_eq!(
            serde_json::to_value(&loaded).unwrap(),
            serde_json::to_value(&document).unwrap()
        );
        save(directory.path(), &loaded).unwrap();
        let saved: serde_json::Value =
            serde_saphyr::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(saved, serde_json::to_value(&document).unwrap());
    }

    #[test]
    fn yaml_contains_only_ids_and_secrets_are_private_and_isolated() {
        let directory = tempfile::tempdir().unwrap();
        let mut document = ConfigDocument::default();
        document.profiles[0].settings.vpn.config = "client\n<key>\nPRIVATE MATERIAL\n</key>".into();
        save(directory.path(), &document).unwrap();
        let content = fs::read_to_string(directory.path().join("config.yaml")).unwrap();
        assert!(!content.contains("PRIVATE MATERIAL"));
        let loaded = load(directory.path()).unwrap();
        assert!(
            loaded.profiles[0]
                .settings
                .vpn
                .key
                .contains("PRIVATE MATERIAL")
        );
        let key = fs::read_dir(directory.path().join("secrets"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            fs::metadata(&key).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(directory.path().join("secrets"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}
