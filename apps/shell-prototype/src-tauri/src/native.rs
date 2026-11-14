//! Native media actor: no decoding, account access or pixel transfer through JavaScript.
use futures::{StreamExt, channel::oneshot};
use lazybuddy_media_ipc::{
    composition::{Compositor, Layout},
    receiver::{Frame, Receiver},
};
use lazybuddy_service_client::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{Emitter, WebviewWindow};

type Reply = oneshot::Sender<Result<Snapshot, String>>;
#[derive(Clone, Serialize)]
pub struct Snapshot {
    session: u64,
    loaded: bool,
    playing: bool,
    eos: bool,
    position: f64,
    duration: f64,
    speed: f64,
    volume: f64,
    mute: bool,
    frames: u64,
    frame_timestamp_ns: u64,
    width: u32,
    height: u32,
    decoder_pid: Option<u32>,
    source_opens: u64,
    seeking: bool,
    error: Option<String>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            session: 0,
            loaded: false,
            playing: false,
            eos: false,
            position: 0.,
            duration: 0.,
            speed: 1.,
            volume: 0.5,
            mute: true,
            frames: 0,
            frame_timestamp_ns: 0,
            width: 0,
            height: 0,
            decoder_pid: None,
            source_opens: 0,
            seeking: false,
            error: None,
        }
    }
}
#[derive(Clone, Copy, Deserialize)]
pub struct Area {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
impl Area {
    fn physical(self, scale: f64) -> Result<Layout, String> {
        if ![self.x, self.y, self.width, self.height, scale]
            .into_iter()
            .all(f64::is_finite)
            || self.x < 0.
            || self.y < 0.
            || self.x > 8192.
            || self.y > 8192.
            || !(0.5..=4.).contains(&scale)
        {
            return Err("invalid_video_area".into());
        }
        let value = Layout {
            x: (self.x * scale).round() as i32,
            y: (self.y * scale).round() as i32,
            width: (self.width * scale).round() as u32,
            height: (self.height * scale).round() as u32,
        };
        value
            .valid()
            .then_some(value)
            .ok_or_else(|| "invalid_video_area".into())
    }
}
#[derive(Default)]
struct Gate {
    page: AtomicBool,
    visible: AtomicBool,
    enabled: AtomicBool,
    epoch: AtomicU64,
    closing: AtomicBool,
}
impl Gate {
    fn live(&self, epoch: u64) -> bool {
        self.page.load(Ordering::Acquire)
            && self.visible.load(Ordering::Acquire)
            && self.enabled.load(Ordering::Acquire)
            && self.epoch.load(Ordering::Acquire) == epoch
    }
}
enum Command {
    Open(u64, Layout, Reply),
    Control(String, Value, Reply),
    Layout(Layout),
    Context(bool, bool),
    Frame(u64, Result<Arc<Frame>, String>),
    End(u64),
    Tick,
    Stop,
    Probe(oneshot::Sender<Result<[u8; 4], String>>),
    Shutdown(oneshot::Sender<()>),
}
struct Controller {
    sender: mpsc::Sender<Command>,
    status: Arc<Mutex<Snapshot>>,
}
#[derive(Default)]
pub struct State {
    gate: Arc<Gate>,
    controller: Mutex<Option<Arc<Controller>>>,
}
impl State {
    fn get(&self, window: &WebviewWindow) -> Result<Arc<Controller>, String> {
        if self.gate.closing.load(Ordering::Acquire) {
            return Err("application_closing".into());
        }
        let mut slot = self.controller.lock().unwrap();
        if let Some(value) = &*slot {
            return Ok(value.clone());
        }
        let hwnd = window.hwnd().map_err(|_| "native_window_unavailable")?.0 as usize;
        let (sender, receiver) = mpsc::channel();
        let status = Arc::new(Mutex::new(Snapshot::default()));
        let controller = Arc::new(Controller {
            sender: sender.clone(),
            status: status.clone(),
        });
        let window = window.clone();
        let gate = self.gate.clone();
        thread::spawn(move || run(window, hwnd, gate, sender, receiver, status));
        *slot = Some(controller.clone());
        Ok(controller)
    }
    pub fn context(&self, page: bool, visible: bool) {
        self.gate.page.store(page, Ordering::Release);
        self.gate.visible.store(visible, Ordering::Release);
        if !page {
            self.gate.enabled.store(false, Ordering::Release);
            self.gate.epoch.fetch_add(1, Ordering::AcqRel);
        }
        if let Some(controller) = self.controller.lock().unwrap().as_ref() {
            let _ = controller.sender.send(Command::Context(page, visible));
        }
    }
    pub fn shutdown(&self) -> Option<impl std::future::Future<Output = ()> + Send + 'static> {
        if self.gate.closing.swap(true, Ordering::AcqRel) {
            return None;
        }
        self.gate.enabled.store(false, Ordering::Release);
        let (sender, receiver) = oneshot::channel();
        if let Some(controller) = self.controller.lock().unwrap().take() {
            let _ = controller.sender.send(Command::Shutdown(sender));
        }
        Some(async move {
            let _ = receiver.await;
        })
    }
}
fn enabled() -> bool {
    std::env::var("LAZYBUDDY_NATIVE_FIXTURE").is_ok_and(|value| value == "1")
}
pub fn mode() -> bool {
    enabled()
}
fn source() -> Result<String, String> {
    if !enabled() {
        return Err("native_source_not_configured".into());
    }
    let value = std::env::var("LAZYBUDDY_NATIVE_FIXTURE_URL")
        .map_err(|_| "native_source_not_configured")?;
    // Only a fixed local synthetic source can be selected by the preview environment.
    let rest = value
        .strip_prefix("http://127.0.0.1:")
        .ok_or("invalid_native_fixture")?;
    let (port, path) = rest.split_once('/').ok_or("invalid_native_fixture")?;
    if port.parse::<u16>().ok().filter(|p| *p != 0).is_none() || path != "sample.mp4" {
        return Err("invalid_native_fixture".into());
    }
    Ok(value)
}
fn runtime() -> Result<(PathBuf, Vec<String>, Vec<(String, String)>), String> {
    let (python, program, root) =
        if let Some(root) = std::env::var_os("LAZYBUDDY_NATIVE_PACKAGES").map(PathBuf::from) {
            let media = root.join("media-runtime");
            for (path, id) in [
                (media.join("module.json"), "media-runtime"),
                (root.join("python-runtime/module.json"), "python-runtime"),
            ] {
                let manifest: lazybuddy_module_contract::Manifest = serde_json::from_slice(
                    &std::fs::read(path).map_err(|_| "media_runtime_unavailable")?,
                )
                .map_err(|_| "invalid_media_runtime")?;
                manifest.validate().map_err(|_| "invalid_media_runtime")?;
                if manifest.id != id {
                    return Err("invalid_media_runtime".into());
                }
            }
            (
                root.join("python-runtime/bin/pythonw.exe"),
                media.join("service/lazybuddy_media.py"),
                media.join("runtime"),
            )
        } else {
            let root =
                PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("media_runtime_unavailable")?)
                    .join("LazyBuddy/modules");
            let manager = lazybuddy_module_manager::Manager::new(root);
            let program = manager
                .entrypoint("media-runtime")
                .map_err(|_| "media_runtime_unavailable")?;
            let runtime = program
                .parent()
                .and_then(|p| p.parent())
                .ok_or("invalid_media_runtime")?
                .join("runtime");
            (
                manager
                    .entrypoint("python-runtime")
                    .map_err(|_| "media_runtime_unavailable")?,
                program,
                runtime,
            )
        };
    let environment = vec![(
        "LAZYBUDDY_GSTREAMER_DIR".into(),
        root.to_string_lossy().into_owned(),
    )];
    Ok((
        python,
        vec![
            "-I".into(),
            "-B".into(),
            program.to_string_lossy().into_owned(),
        ],
        environment,
    ))
}
fn pipe_name() -> Result<String, String> {
    use windows_sys::Win32::Security::Cryptography::*;
    let mut bytes = [0u8; 16];
    if unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    } < 0
    {
        return Err("media_pipe_failed".into());
    }
    Ok(format!(
        r"\\.\pipe\lazybuddy-media-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
fn request(client: &mut Client, method: &str, args: Value) -> Result<Value, String> {
    let reply = client
        .request(&json!({"version":1,"id":"native-media","method":method,"args":args}))
        .map_err(|_| "media_service_unavailable")?;
    if reply["ok"] != true {
        return Err(reply["error"]["code"]
            .as_str()
            .filter(|code| {
                code.len() < 80 && code.bytes().all(|c| c.is_ascii_lowercase() || c == b'_')
            })
            .unwrap_or("media_service_failed")
            .into());
    }
    Ok(reply["data"].clone())
}
struct Media {
    client: Option<Client>,
    receiver: Option<Receiver>,
    forward: Option<thread::JoinHandle<()>>,
    compositor: Option<Compositor>,
    layout: Layout,
    opacity: f32,
    epoch: u64,
    want_frame: bool,
    intent_playing: bool,
    seek_target: Option<f64>,
    last_event: Instant,
}
impl Media {
    fn stop(&mut self) {
        self.receiver.take();
        if let Some(mut client) = self.client.take() {
            let _ = request(&mut client, "close", json!({}));
        }
        if let Some(forward) = self.forward.take() {
            let _ = forward.join();
        }
        self.compositor.take();
    }
}
impl Drop for Media {
    fn drop(&mut self) {
        self.stop();
    }
}
fn emit(window: &WebviewWindow, gate: &Gate, status: &Snapshot) {
    if gate.live(status.session) {
        let _ = window.emit("native-media-state", status);
    }
}
fn stop_failed(
    media: &mut Option<Media>,
    status: &mut Snapshot,
    gate: &Gate,
    window: &WebviewWindow,
    error: String,
) {
    gate.enabled.store(false, Ordering::Release);
    media.take();
    status.loaded = false;
    status.playing = false;
    status.decoder_pid = None;
    status.error = Some(error);
    if gate.page.load(Ordering::Acquire) && gate.visible.load(Ordering::Acquire) {
        let _ = window.emit("native-media-state", &*status);
    }
}
fn update(status: &mut Snapshot, data: &Value) {
    status.loaded = data["loaded"].as_bool().unwrap_or(false);
    status.eos = data["eos"].as_bool().unwrap_or(false);
    status.playing = data["playing"].as_bool().unwrap_or(false) && !status.eos;
    if !status.seeking {
        status.position = data["position"].as_f64().unwrap_or(status.position);
    }
    status.duration = data["duration"].as_f64().unwrap_or(status.duration);
    status.width = data["decoded_width"]
        .as_u64()
        .unwrap_or(status.width.into()) as u32;
    status.height = data["decoded_height"]
        .as_u64()
        .unwrap_or(status.height.into()) as u32;
    if data["error"].is_string() {
        status.error = Some("media_decode_failed".into());
    }
}
fn run(
    window: WebviewWindow,
    hwnd: usize,
    gate: Arc<Gate>,
    sender: mpsc::Sender<Command>,
    commands: mpsc::Receiver<Command>,
    shared: Arc<Mutex<Snapshot>>,
) {
    let mut status = Snapshot::default();
    let mut media: Option<Media> = None;
    loop {
        // Frames can stop before bus EOS. Poll only during visible playback.
        let command = if media
            .as_ref()
            .is_some_and(|m| m.intent_playing && gate.live(m.epoch))
        {
            match commands.recv_timeout(Duration::from_millis(250)) {
                Ok(command) => command,
                Err(mpsc::RecvTimeoutError::Timeout) => Command::Tick,
                Err(_) => break,
            }
        } else {
            match commands.recv() {
                Ok(command) => command,
                Err(_) => break,
            }
        };
        match command {
            Command::Open(epoch, layout, reply) => {
                media.take();
                status.loaded = false;
                status.playing = false;
                status.error = None;
                status.session = epoch;
                status.frames = 0;
                status.seeking = false;
                let result = (|| -> Result<Snapshot, String> {
                    if !gate.live(epoch) {
                        return Err("inactive_media_page".into());
                    }
                    let uri = source()?;
                    let (python, args, environment) = runtime()?;
                    let mut client = Client::start_with_environment(&python, &args, &environment)
                        .map_err(|_| "media_runtime_unavailable")?;
                    let pipe = pipe_name()?;
                    let health = request(&mut client, "health", json!({}))?;
                    if health["capabilities"]["paused_frames"] != true {
                        return Err("media_runtime_upgrade_required".into());
                    }
                    request(&mut client, "open", json!({"uri":uri,"pipe":pipe}))?;
                    request(&mut client, "mute", json!({"value":status.mute}))?;
                    request(&mut client, "volume", json!({"value":status.volume}))?;
                    let (receiver, mut frames) = Receiver::start(pipe, client.pid());
                    let sink = sender.clone();
                    let forward = thread::spawn(move || {
                        futures::executor::block_on(async move {
                            while let Some(frame) = frames.next().await {
                                if sink.send(Command::Frame(epoch, frame)).is_err() {
                                    return;
                                }
                            }
                            let _ = sink.send(Command::End(epoch));
                        })
                    });
                    status.decoder_pid = Some(client.pid());
                    status.source_opens += 1;
                    let mut value = Media {
                        client: Some(client),
                        receiver: Some(receiver),
                        forward: Some(forward),
                        compositor: None,
                        layout,
                        opacity: 1.,
                        epoch,
                        want_frame: true,
                        intent_playing: false,
                        seek_target: None,
                        last_event: Instant::now() - Duration::from_secs(1),
                    };
                    if status.speed != 1. {
                        request(
                            value.client.as_mut().unwrap(),
                            "speed",
                            json!({"value":status.speed}),
                        )?;
                    }
                    // IPC timestamps are system presentation times, not media positions.
                    if status.position > 0.1 {
                        request(
                            value.client.as_mut().unwrap(),
                            "seek",
                            json!({"value":status.position}),
                        )?;
                        value.seek_target = Some(status.position);
                        status.seeking = true;
                    }
                    let data = request(value.client.as_mut().unwrap(), "status", json!({}))?;
                    update(&mut status, &data);
                    media = Some(value);
                    Ok(status.clone())
                })();
                if let Err(error) = &result {
                    status.error = Some(error.clone());
                    status.decoder_pid = None;
                }
                let _ = reply.send(result);
            }
            Command::Frame(epoch, frame) => {
                if !gate.live(epoch) {
                    if let Ok(frame) = frame {
                        frame.release();
                    }
                    continue;
                }
                if let Some(current) = media.as_mut().filter(|m| m.epoch == epoch) {
                    let result = (|| -> Result<(), String> {
                        let frame = frame.map_err(|_| "media_texture_transport_failed")?;
                        if !current.intent_playing && !current.want_frame {
                            frame.release();
                            return Ok(());
                        }
                        if current.compositor.is_none() {
                            current.compositor = Some(
                                unsafe {
                                    Compositor::new(hwnd, frame.adapter_luid, current.layout)
                                }
                                .map_err(|error| {
                                    if std::env::var_os("LAZYBUDDY_PROTOTYPE_PROBE").is_some() {
                                        eprintln!("native composition: {error}");
                                    }
                                    "native_composition_failed"
                                })?,
                            );
                        }
                        let compositor = current.compositor.as_mut().unwrap();
                        compositor
                            .frame(&frame)
                            .map_err(|_| "native_frame_copy_failed")?;
                        if !gate.live(epoch) {
                            return Ok(());
                        }
                        compositor
                            .present(current.opacity, false)
                            .map_err(|_| "native_present_failed")?;
                        status.frames += 1;
                        // GStreamer's IPC clock is an absolute presentation time.
                        status.frame_timestamp_ns = frame.pts_ns;
                        if current.seek_target.is_some() {
                            current.seek_target = None;
                            status.seeking = false;
                        }
                        current.want_frame = false;
                        if current.last_event.elapsed() >= Duration::from_millis(200)
                            || !status.playing
                        {
                            let data =
                                request(current.client.as_mut().unwrap(), "status", json!({}))?;
                            update(&mut status, &data);
                            if status.error.is_some() {
                                return Err("media_decode_failed".into());
                            }
                            emit(&window, &gate, &status);
                            current.last_event = Instant::now();
                        }
                        Ok(())
                    })();
                    if let Err(error) = result {
                        stop_failed(&mut media, &mut status, &gate, &window, error);
                    }
                }
            }
            Command::Control(method, value, reply) => {
                let result = (|| {
                    let current = media.as_mut().ok_or("media_not_loaded")?;
                    if method != "pause" && !gate.live(current.epoch) {
                        return Err("inactive_media_page".into());
                    }
                    match method.as_str() {
                        "opacity" => {
                            let opacity = value.as_f64().ok_or("invalid_opacity")? as f32;
                            if !opacity.is_finite() || !(0. ..=1.).contains(&opacity) {
                                return Err("invalid_opacity".into());
                            }
                            current.opacity = opacity;
                            if let Some(renderer) = &current.compositor {
                                renderer
                                    .present(current.opacity, false)
                                    .map_err(|_| "native_present_failed")?;
                            }
                        }
                        "play" | "pause" | "seek" | "speed" | "volume" | "mute" => {
                            let args = if method == "play" || method == "pause" {
                                json!({})
                            } else {
                                json!({"value":value})
                            };
                            let data = request(current.client.as_mut().unwrap(), &method, args)?;
                            if method == "play" {
                                status.playing = true;
                                current.want_frame = true;
                                current.intent_playing = true;
                            }
                            if method == "pause" {
                                status.playing = false;
                                current.want_frame = false;
                                current.intent_playing = false;
                            }
                            if method == "seek" {
                                let target = value.as_f64().ok_or("invalid_seek")?;
                                current.seek_target = Some(target);
                                status.position = target;
                                status.seeking = true;
                                current.want_frame = true;
                            }
                            if method == "speed" {
                                status.speed = value.as_f64().ok_or("invalid_speed")?;
                                current.want_frame = true;
                            }
                            if method == "volume" {
                                status.volume = value.as_f64().ok_or("invalid_volume")?;
                            }
                            if method == "mute" {
                                status.mute = value.as_bool().ok_or("invalid_mute")?;
                            }
                            let intent = status.playing;
                            update(&mut status, &data);
                            if method == "play" {
                                status.playing = intent;
                            }
                        }
                        _ => return Err("unknown_media_control".into()),
                    }
                    emit(&window, &gate, &status);
                    Ok(status.clone())
                })();
                let _ = reply.send(result);
            }
            Command::Layout(layout) => {
                let mut error = false;
                if let Some(current) = media.as_mut() {
                    current.layout = layout;
                    if gate.live(current.epoch) {
                        if let Some(renderer) = current.compositor.as_mut() {
                            error = renderer
                                .layout(layout)
                                .and_then(|_| renderer.present(current.opacity, false).map(|_| ()))
                                .is_err();
                        }
                    }
                }
                if error {
                    stop_failed(
                        &mut media,
                        &mut status,
                        &gate,
                        &window,
                        "native_layout_failed".into(),
                    );
                }
            }
            Command::Context(page, visible) => {
                if !page {
                    media.take();
                    status.loaded = false;
                    status.playing = false;
                    status.decoder_pid = None;
                } else if let Some(current) = media.as_mut() {
                    let result = (|| -> Result<(), String> {
                        if !visible {
                            let data =
                                request(current.client.as_mut().unwrap(), "pause", json!({}))?;
                            update(&mut status, &data);
                            status.playing = false;
                            current.want_frame = false;
                            current.intent_playing = false;
                        }
                        if let Some(renderer) = current.compositor.as_ref() {
                            renderer
                                .visible(visible)
                                .map_err(|_| "native_visibility_failed")?;
                            if visible {
                                renderer
                                    .present(current.opacity, false)
                                    .map_err(|_| "native_present_failed")?;
                            }
                        }
                        if visible {
                            emit(&window, &gate, &status);
                        }
                        Ok(())
                    })();
                    if let Err(error) = result {
                        stop_failed(&mut media, &mut status, &gate, &window, error);
                    }
                }
            }
            Command::Stop => {
                media.take();
                status.loaded = false;
                status.playing = false;
                status.decoder_pid = None;
            }
            Command::End(epoch) => {
                if let Some(current) = media.as_mut().filter(|m| m.epoch == epoch) {
                    if let Ok(data) = request(current.client.as_mut().unwrap(), "status", json!({}))
                    {
                        update(&mut status, &data);
                        emit(&window, &gate, &status);
                    }
                }
            }
            Command::Probe(reply) => {
                let value = media
                    .as_ref()
                    .and_then(|m| m.compositor.as_ref().map(|r| (m, r)))
                    .ok_or_else(|| "native_frame_unavailable".to_string())
                    .and_then(|(m, r)| {
                        r.present(m.opacity, true)
                            .map_err(|_| "native_probe_failed".to_string())?
                            .ok_or_else(|| "native_frame_unavailable".into())
                    });
                let _ = reply.send(value);
            }
            Command::Tick => {
                if let Some(current) = media
                    .as_mut()
                    .filter(|m| gate.live(m.epoch) && m.intent_playing)
                {
                    match request(current.client.as_mut().unwrap(), "status", json!({})) {
                        Ok(data) => {
                            update(&mut status, &data);
                            if status.error.is_some() {
                                stop_failed(
                                    &mut media,
                                    &mut status,
                                    &gate,
                                    &window,
                                    "media_decode_failed".into(),
                                );
                            } else {
                                if status.eos {
                                    current.intent_playing = false;
                                }
                                emit(&window, &gate, &status);
                            }
                        }
                        Err(error) => stop_failed(&mut media, &mut status, &gate, &window, error),
                    }
                }
            }
            Command::Shutdown(reply) => {
                media.take();
                let _ = reply.send(());
                break;
            }
        }
        *shared.lock().unwrap() = status.clone();
    }
}
#[tauri::command]
pub async fn native_open(
    area: Area,
    window: WebviewWindow,
    state: tauri::State<'_, State>,
) -> Result<Snapshot, String> {
    if !enabled() {
        return Err("native_preview_not_enabled".into());
    }
    let layout = area.physical(
        window
            .scale_factor()
            .map_err(|_| "native_window_unavailable")?,
    )?;
    if !state.gate.page.load(Ordering::Acquire) || !state.gate.visible.load(Ordering::Acquire) {
        return Err("inactive_media_page".into());
    }
    let controller = state.get(&window)?;
    state.gate.enabled.store(true, Ordering::Release);
    let epoch = state.gate.epoch.fetch_add(1, Ordering::AcqRel) + 1;
    let (sender, receiver) = oneshot::channel();
    controller
        .sender
        .send(Command::Open(epoch, layout, sender))
        .map_err(|_| "media_actor_stopped")?;
    receiver.await.map_err(|_| "media_actor_stopped")?
}
#[tauri::command]
pub async fn native_control(
    method: String,
    value: Option<Value>,
    state: tauri::State<'_, State>,
) -> Result<Snapshot, String> {
    let controller = state
        .controller
        .lock()
        .unwrap()
        .clone()
        .ok_or("media_not_loaded")?;
    let (sender, receiver) = oneshot::channel();
    controller
        .sender
        .send(Command::Control(
            method,
            value.unwrap_or(Value::Null),
            sender,
        ))
        .map_err(|_| "media_actor_stopped")?;
    receiver.await.map_err(|_| "media_actor_stopped")?
}
#[tauri::command]
pub fn native_layout(
    area: Area,
    window: WebviewWindow,
    state: tauri::State<State>,
) -> Result<(), String> {
    let layout = area.physical(
        window
            .scale_factor()
            .map_err(|_| "native_window_unavailable")?,
    )?;
    if let Some(controller) = state.controller.lock().unwrap().as_ref() {
        let _ = controller.sender.send(Command::Layout(layout));
    }
    Ok(())
}
#[tauri::command]
pub fn native_snapshot(state: tauri::State<State>) -> Snapshot {
    state
        .controller
        .lock()
        .unwrap()
        .as_ref()
        .map(|c| c.status.lock().unwrap().clone())
        .unwrap_or_default()
}
#[tauri::command]
pub fn native_close(state: tauri::State<State>) {
    state.gate.enabled.store(false, Ordering::Release);
    state.gate.epoch.fetch_add(1, Ordering::AcqRel);
    if let Some(controller) = state.controller.lock().unwrap().as_ref() {
        let _ = controller.sender.send(Command::Stop);
    }
}
#[tauri::command]
pub async fn native_probe(state: tauri::State<'_, State>) -> Result<[u8; 4], String> {
    if std::env::var_os("LAZYBUDDY_PROTOTYPE_PROBE").is_none() || !enabled() {
        return Err("native_probe_disabled".into());
    }
    let controller = state
        .controller
        .lock()
        .unwrap()
        .clone()
        .ok_or("media_not_loaded")?;
    let (sender, receiver) = oneshot::channel();
    controller
        .sender
        .send(Command::Probe(sender))
        .map_err(|_| "media_actor_stopped")?;
    receiver.await.map_err(|_| "media_actor_stopped")?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eos_overrides_pipeline_playing_without_using_ipc_clock_as_progress() {
        let mut status = Snapshot {
            frame_timestamp_ns: 123_456_000_000,
            ..Default::default()
        };
        update(
            &mut status,
            &json!({"loaded":true,"playing":true,"eos":true,"position":3.5}),
        );
        assert!(status.eos);
        assert!(!status.playing);
        assert_eq!(status.position, 3.5);
        assert_eq!(status.frame_timestamp_ns, 123_456_000_000);
    }
    #[test]
    fn inactive_or_old_sessions_cannot_present() {
        let gate = Gate::default();
        gate.page.store(true, Ordering::Relaxed);
        gate.visible.store(true, Ordering::Relaxed);
        gate.enabled.store(true, Ordering::Relaxed);
        gate.epoch.store(3, Ordering::Relaxed);
        assert!(gate.live(3));
        assert!(!gate.live(2));
        gate.visible.store(false, Ordering::Relaxed);
        assert!(!gate.live(3));
    }
    #[test]
    fn layout_checks_finite_values_and_dpi_bounds() {
        assert!(
            Area {
                x: 0.,
                y: 62.,
                width: 780.,
                height: 400.
            }
            .physical(1.5)
            .is_ok()
        );
        assert!(
            Area {
                x: 0.,
                y: 0.,
                width: f64::NAN,
                height: 400.
            }
            .physical(1.)
            .is_err()
        );
        assert!(
            Area {
                x: 0.,
                y: 0.,
                width: 9000.,
                height: 400.
            }
            .physical(1.)
            .is_err()
        );
    }
}
