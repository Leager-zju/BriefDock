//! Private media session control. Compressed resources go straight to WebView.
use lazybuddy_service_client::Session;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tauri::Manager;

struct Connection {
    service: Session,
    lease: Option<u64>,
    account_pending: bool,
}
#[derive(Default)]
pub struct State {
    connection: Mutex<Option<Connection>>,
    epoch: AtomicU64,
    closing: AtomicBool,
    view: Mutex<(bool, bool, bool)>,
}
pub fn enabled() -> bool {
    std::env::var_os("LAZYBUDDY_BROWSER_PACKAGES").is_some()
}
fn scoped_package(version: &str) -> bool {
    let Ok(parts) = version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
    else {
        return false;
    };
    parts.len() == 3 && parts.as_slice() >= &[0, 2, 1]
}
fn connect() -> Result<Session, String> {
    // Explicit development package directory; never load sibling service source.
    let root = PathBuf::from(
        std::env::var_os("LAZYBUDDY_BROWSER_PACKAGES").ok_or("browser_service_not_enabled")?,
    );
    let read = |id: &str| -> Result<lazybuddy_module_contract::Manifest, String> {
        let manifest: lazybuddy_module_contract::Manifest = serde_json::from_slice(
            &std::fs::read(root.join(id).join("module.json"))
                .map_err(|_| "service_package_unavailable")?,
        )
        .map_err(|_| "invalid_service_package")?;
        manifest.validate().map_err(|_| "invalid_service_package")?;
        if manifest.id != id {
            return Err("invalid_service_package".into());
        }
        Ok(manifest)
    };
    let feature = read("hongguo")?;
    if !scoped_package(&feature.version) {
        return Err("service_package_upgrade_required".into());
    }
    let python = read("python-runtime")?;
    let mut arguments = vec![
        "-I".into(),
        "-B".into(),
        root.join("hongguo")
            .join(feature.entrypoint)
            .to_string_lossy()
            .into_owned(),
    ];
    arguments.extend(feature.arguments);
    Ok(Session::with_environment(
        root.join("python-runtime").join(python.entrypoint),
        arguments,
        vec![
            ("LAZYBUDDY_INSTANCE".into(), "shell-preview".into()),
            ("LAZYBUDDY_CREDENTIAL_SCOPE".into(), "shell-preview".into()),
            (
                "LAZYBUDDY_DATA_DIR".into(),
                PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("local_data_unavailable")?)
                    .join("LazyBuddyShellPreview/hongguo")
                    .to_string_lossy()
                    .into_owned(),
            ),
            ("LAZYBUDDY_PLAYER_REAL".into(), "1".into()),
            ("LAZYBUDDY_SEARCH_REAL".into(), "1".into()),
            ("LAZYBUDDY_FAVORITES_REAL".into(), "1".into()),
            ("LAZYBUDDY_PROGRESS_REAL".into(), "0".into()),
        ],
    ))
}
impl State {
    pub fn context(&self, page: bool, account: bool, visible: bool) {
        let previous = {
            let mut view = self.view.lock().unwrap();
            let previous = *view;
            *view = (page, account, visible);
            previous
        };
        if previous == (page, account, visible) {
            return;
        }
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let mut slot = self.connection.lock().unwrap();
        if (page || account) && visible {
            if previous.0 != page || previous.1 != account {
                if slot
                    .as_ref()
                    .is_some_and(|connection| connection.lease.is_none())
                {
                    if let Some(connection) = slot.take() {
                        if connection.account_pending {
                            tauri::async_runtime::spawn(connection.service.shutdown());
                        } else {
                            connection.service.stop();
                        }
                    }
                }
            }
            if !page {
                if let Some(connection) = slot.as_mut() {
                    if let Some(lease) = connection.lease.take() {
                        self.epoch.fetch_add(1, Ordering::AcqRel);
                        let future=connection.service.request(json!({"version":1,"id":"leave-player","method":"playback.close","args":{"session":lease}}));
                        tauri::async_runtime::spawn(async move {
                            let _ = future.await;
                        });
                    }
                }
            }
            return;
        }
        if !page && !account {
            if let Some(connection) = slot.take() {
                if connection.lease.is_none() && !connection.account_pending {
                    connection.service.stop();
                } else {
                    tauri::async_runtime::spawn(connection.service.shutdown());
                }
            }
        } else if let Some(connection) = slot.as_ref() {
            if let Some(lease) = connection.lease {
                let service = connection.service.clone();
                tauri::async_runtime::spawn(async move {
                    if !service.request(json!({"version":1,"id":"visibility","method":"playback.suspend","args":{"session":lease}})).await.is_ok_and(|reply|reply["ok"]==true) {service.stop();}
                });
            } else if let Some(connection) = slot.take() {
                connection.service.stop();
            }
        }
    }
    pub fn shutdown(&self) -> Option<impl std::future::Future<Output = ()> + Send + 'static> {
        if self.closing.swap(true, Ordering::AcqRel) {
            return None;
        }
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let service = self.connection.lock().unwrap().take().map(|c| c.service);
        Some(async move {
            if let Some(service) = service {
                service.shutdown().await;
            }
        })
    }
}
fn allowed(method: &str) -> bool {
    matches!(
        method,
        "playback.open"
            | "playback.status"
            | "playback.close"
            | "playback.suspend"
            | "playback.resume"
            | "playback.checkpoint"
            | "account.restore"
            | "account.status"
            | "account.login"
            | "account.cancel"
            | "account.logout"
            | "search"
            | "favorites.read"
            | "favorites.set"
            | "progress.read"
            | "cover.read"
    )
}
#[tauri::command]
pub async fn playback_call(
    method: String,
    args: Value,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, State>,
) -> Result<Value, String> {
    if !allowed(&method) || !args.is_object() {
        return Err("invalid_playback_request".into());
    }
    if state.closing.load(Ordering::Acquire) {
        return Err("application_closing".into());
    }
    let checkpoint = method == "playback.checkpoint";
    let account_method = method.starts_with("account.");
    {
        let model = window.state::<Mutex<crate::Model>>();
        let model = model.lock().unwrap();
        let content = model.view.section == crate::Section::Content;
        let account_settings =
            model.view.section == crate::Section::Settings && model.account_page && account_method;
        if model.view.root != crate::Root::Hongguo
            || (!(content || account_settings) && !checkpoint)
            || (!model.view.visible && !checkpoint)
        {
            return Err("inactive_media_page".into());
        }
    }
    let epoch = state.epoch.load(Ordering::Acquire);
    let service = {
        let mut slot = state.connection.lock().unwrap();
        if slot.is_none() {
            if checkpoint {
                return Err("stale_playback_request".into());
            }
            *slot = Some(Connection {
                service: connect()?,
                lease: None,
                account_pending: false,
            });
        }
        slot.as_ref().unwrap().service.clone()
    };
    let reply = service
        .request(json!({"version":1,"id":"web-playback","method":method,"args":args}))
        .await;
    if epoch != state.epoch.load(Ordering::Acquire) {
        return Err("stale_playback_request".into());
    }
    let reply = match reply {
        Ok(value) => value,
        Err(_) => {
            if let Some(connection) = state.connection.lock().unwrap().take() {
                connection.service.stop();
            }
            return Err("playback_service_unavailable".into());
        }
    };
    if reply["ok"] != true {
        return Err(reply["error"]["code"]
            .as_str()
            .filter(|s| s.len() < 80 && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
            .unwrap_or("playback_failed")
            .to_owned());
    }
    if let Some(connection) = state.connection.lock().unwrap().as_mut() {
        if method.starts_with("playback.") {
            connection.lease = resource_session(&reply["data"]);
        }
        if account_method || method.starts_with("favorites.") {
            connection.account_pending = reply["data"]["operation"] == "pending";
        }
    }
    Ok(reply["data"].clone())
}
fn resource_session(data: &Value) -> Option<u64> {
    if data["loading"] == true || data["prepared"] == true {
        data["session"].as_u64()
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_resource_does_not_keep_a_decoder_lease() {
        assert_eq!(
            resource_session(&json!({"session":42,"prepared":false,"loading":false})),
            None
        );
        assert_eq!(
            resource_session(&json!({"session":42,"loading":true})),
            Some(42)
        );
    }
    #[test]
    fn browser_bridge_only_exposes_media_session_operations() {
        assert!(allowed("playback.open"));
        assert!(allowed("account.login"));
        assert!(!allowed("media.open"));
        assert!(!allowed("progress.sync"));
    }
    #[test]
    fn old_packages_cannot_silently_use_production_credentials() {
        assert!(!scoped_package("0.2.0"));
        assert!(scoped_package("0.2.1"));
        assert!(scoped_package("0.10.0"));
        assert!(!scoped_package("bad"));
    }
}
