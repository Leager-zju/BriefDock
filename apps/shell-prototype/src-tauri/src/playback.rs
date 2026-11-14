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
}
#[derive(Default)]
pub struct State {
    connection: Mutex<Option<Connection>>,
    epoch: AtomicU64,
    closing: AtomicBool,
}
pub fn enabled() -> bool {
    std::env::var_os("LAZYBUDDY_BROWSER_PACKAGES").is_some()
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
        vec![],
    ))
}
impl State {
    pub fn context(&self, page: bool, visible: bool) {
        if page && visible {
            return;
        }
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let mut slot = self.connection.lock().unwrap();
        if !page {
            if let Some(connection) = slot.take() {
                tauri::async_runtime::spawn(connection.service.shutdown());
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
    {
        let model = window.state::<Mutex<crate::Model>>();
        let model = model.lock().unwrap();
        if model.view.root != crate::Root::Hongguo
            || model.view.section != crate::Section::Content
            || !model.view.visible
        {
            return Err("inactive_media_page".into());
        }
    }
    let epoch = state.epoch.load(Ordering::Acquire);
    let service = {
        let mut slot = state.connection.lock().unwrap();
        if slot.is_none() {
            *slot = Some(Connection {
                service: connect()?,
                lease: None,
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
        connection.lease = reply["data"]["session"].as_u64();
    }
    Ok(reply["data"].clone())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_bridge_only_exposes_media_session_operations() {
        assert!(allowed("playback.open"));
        assert!(!allowed("account.login"));
        assert!(!allowed("media.open"));
    }
}
