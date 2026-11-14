#![windows_subsystem = "windows"]
#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
compile_error!("LazyBuddy desktop currently supports Windows x64 only");
mod control;
mod input;
mod lifecycle;
mod modules;
mod platform;
mod preferences;
mod results;
mod shortcut;
mod startup;
mod state;
mod view;

use iced::{Element, Subscription, Task};
use lazybuddy_module_contract::{Activation, Kind, Manifest, Page};
use lazybuddy_module_manager::Manager;
use preferences::Preferences;
use serde_json::{Value, json};
use state::{PageState, Section, Settings};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Root {
    id: String,
    name: String,
}
impl std::fmt::Display for Root {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Clone, Debug)]
enum Message {
    ControlsHover(bool),
    ControlsMenu(bool),
    Keyboard(iced::keyboard::Event, iced::event::Status),
    HoldTick,
    Blur,
    CaptureShortcut,
    CancelShortcut,
    Shortcut(shortcut::Shortcut),
    Autostart(bool),
    Select(Root),
    Navigate(Section),
    Fetch(String),
    Search,
    More,
    Query(String),
    Favorites(bool),
    Favorite(bool),
    Account(&'static str),
    Poll,
    Scroll(String, u64, iced::widget::scrollable::Viewport),
    Result(String, u64, String, Result<Value, String>),
    Dark(bool),
    Topmost(bool),
    BackgroundOpacity(f32),
    CardOpacity(f32),
    SavePreferences,
    Drag,
    DefaultFavorites(bool),
    Uninstall(String),
    Removed(Result<(), String>),
    Reload,
    Saved(Result<(), String>),
    Play(String, u32),
    Player(&'static str, Value),
    Frame(
        String,
        u64,
        Result<std::sync::Arc<lazybuddy_media_ipc::receiver::Frame>, String>,
    ),
    VideoTick,
    SeekPreview(f64),
    SeekCommit,
    Quality(String),
    VolumePreview(f64),
    VolumeCommit,
    Refresh,
    NewsTick,
    NewsPreferences(Value),
    Followed(bool),
    Category(String),
    Resized(iced::Size),
    Geometry,
    SaveGeometry,
    Dock(Option<platform::Edge>),
    Expand,
    OpenLink(String),
    CatalogLoaded(Result<lazybuddy_module_manager::package::Catalog, String>),
    Install(String),
    Ready,
    Window(Option<iced::window::Id>),
    NativeHandle(usize),
    Visibility(bool),
    ToggleWindow,
    ShowWindow,
    HideWindow,
    Suspended(u64, bool),
    Quit,
    Exit,
    Rpc(control::Incoming),
    Capture(control::Incoming, iced::window::Screenshot),
}

struct App {
    manager: Manager,
    modules: Vec<Manifest>,
    roots: Vec<Root>,
    activation: Activation,
    selected: Option<Root>,
    section: Section,
    pages: BTreeMap<String, PageState>,
    busy: bool,
    error: Option<String>,
    session: Option<lazybuddy_service_client::Session>,
    preferences: Preferences,
    preferences_writer: preferences::Writer,
    preferences_valid: bool,
    frame: Option<std::sync::Arc<lazybuddy_media_ipc::receiver::Frame>>,
    video_receiver: Option<lazybuddy_media_ipc::receiver::Receiver>,
    play_generation: u64,
    width: f32,
    current_method: String,
    queued_requests: std::collections::VecDeque<(String, Value)>,
    request_query: Option<String>,
    catalog: Option<lazybuddy_module_manager::package::Catalog>,
    manager_busy: bool,
    installed: BTreeMap<String, String>,
    desktop: Option<platform::Desktop>,
    hidden: bool,
    capsule: bool,
    shortcut_recording: bool,
    hold: input::Hold,
    autostart: bool,
    quitting: bool,
    closing_sessions: Vec<futures::future::Shared<futures::future::BoxFuture<'static, ()>>>,
}

fn background<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> impl std::future::Future<Output = T> + Send {
    let (sender, receiver) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    async move { receiver.await.expect("background operation must not panic") }
}

impl App {
    fn new() -> Self {
        let root = std::env::var_os("LAZYBUDDY_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(
                    std::env::var_os("LOCALAPPDATA").expect("Windows local app data directory"),
                )
                .join("LazyBuddy")
            });
        assert!(root.is_absolute(), "absolute data directory required");
        let manager = Manager::new(root.join("modules"));
        let preferences_path = root.join("preferences.json");
        let (preferences, preferences_valid, error) = match Preferences::load(&preferences_path) {
            Ok(preferences) => (preferences, true, None),
            Err(_) => (
                Preferences::default(),
                false,
                Some("设置读取失败，已保留原文件。".into()),
            ),
        };
        let preferences_writer = preferences::Writer::new(preferences_path);
        let mut app = Self {
            manager,
            modules: vec![],
            roots: vec![],
            activation: Activation::default(),
            selected: None,
            section: Section::Content,
            pages: BTreeMap::new(),
            busy: false,
            error,
            session: None,
            preferences,
            preferences_writer,
            preferences_valid,
            frame: None,
            video_receiver: None,
            play_generation: 0,
            width: 780.,
            current_method: String::new(),
            queued_requests: Default::default(),
            request_query: None,
            catalog: None,
            manager_busy: false,
            installed: BTreeMap::new(),
            desktop: None,
            hidden: std::env::args().any(|arg| arg == "--background"),
            capsule: false,
            shortcut_recording: false,
            hold: Default::default(),
            autostart: startup::enabled(),
            quitting: false,
            closing_sessions: vec![],
        };
        app.reload();
        if let Some(root) = app
            .preferences
            .root
            .as_ref()
            .and_then(|id| app.roots.iter().find(|root| &root.id == id))
            .cloned()
        {
            let favorites = app
                .preferences
                .pages
                .get(&root.id)
                .is_some_and(|page| page.default_favorites);
            app.pages.insert(
                root.id.clone(),
                PageState {
                    favorites_tab: favorites,
                    ..Default::default()
                },
            );
            app.selected = Some(root);
        }
        app
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ControlsHover(hover) => {
                if let Some(root) = &self.selected {
                    let page = self.pages.get_mut(&root.id).unwrap();
                    page.controls_hover = hover;
                    page.controls_keyboard = false;
                }
            }
            Message::ControlsMenu(open) => {
                if let Some(root) = &self.selected {
                    self.pages.get_mut(&root.id).unwrap().controls_menu = open;
                }
            }
            Message::Keyboard(event, status) => return self.keyboard(event, status),
            Message::Blur => {
                if self.hold.cancel() {
                    return self.request("media.boost".into(), json!({"value":false}));
                }
            }
            Message::HoldTick => {
                let playing = self
                    .page()
                    .is_some_and(|page| page.play_intent && page.media["playing"] == true);
                if self.hold.tick(std::time::Instant::now(), playing) {
                    return self.request("media.boost".into(), json!({"value":true}));
                }
            }
            Message::CaptureShortcut if !lazybuddy_host_control::isolated() => {
                self.shortcut_recording = true;
                if let Some(desktop) = &self.desktop {
                    desktop.capture_shortcut(true);
                }
            }
            Message::CancelShortcut => {
                self.cancel_shortcut();
            }
            Message::Shortcut(shortcut) if self.shortcut_recording => {
                if self
                    .desktop
                    .as_mut()
                    .is_some_and(|desktop| desktop.set_shortcut(shortcut))
                {
                    self.preferences.shortcut = shortcut;
                    self.cancel_shortcut();
                    return self.save();
                }
                self.error = Some("快捷键不可用，请选择其他组合。".into());
            }
            Message::Dock(edge) if !self.capsule && !self.hidden => {
                if self
                    .desktop
                    .as_mut()
                    .is_some_and(|desktop| desktop.dock(edge))
                {
                    self.capsule = true;
                    return self.suspend_content();
                }
            }
            Message::Expand if self.capsule => {
                if self
                    .desktop
                    .as_mut()
                    .is_some_and(|desktop| desktop.expand())
                {
                    self.capsule = false;
                    if self.video_receiver.is_none() {
                        return self.resume();
                    }
                }
            }
            Message::Capture(call, screenshot) => return Self::save_capture(call, screenshot),
            Message::Favorite(value) if self.can_interact() => {
                if let Some(root) = &self.selected {
                    let page = self.pages.get_mut(&root.id).unwrap();
                    page.controls_menu = false;
                    if page.favorite_override.is_none() && page.account["operation"] != "pending" {
                        if let Some(series) = page.media["series"].as_str().map(str::to_owned) {
                            page.favorite_override = Some(value);
                            return self.request(
                                "favorites.set".into(),
                                json!({"series":series,"favorite":value}),
                            );
                        }
                    }
                }
            }
            Message::Autostart(value) => match startup::set(value) {
                Ok(()) => self.autostart = value,
                Err(_) => self.error = Some("开机启动设置失败。".into()),
            },
            Message::Drag => return iced::window::latest().and_then(iced::window::drag),
            Message::Topmost(value) => {
                self.preferences.topmost = value;
                return Task::batch([
                    self.save(),
                    iced::window::latest().and_then(move |id| {
                        iced::window::set_level(
                            id,
                            if value {
                                iced::window::Level::AlwaysOnTop
                            } else {
                                iced::window::Level::Normal
                            },
                        )
                    }),
                ]);
            }
            Message::BackgroundOpacity(value) => {
                self.preferences.background_opacity = value.clamp(0.0, 1.)
            }
            Message::CardOpacity(value) => self.preferences.card_opacity = value.clamp(0.0, 1.),
            Message::SavePreferences => return self.save(),
            Message::Ready => {
                if control::start().is_err() {
                    self.error = Some("本地控制接口启动失败。".into());
                }
                return iced::window::latest().map(Message::Window);
            }
            Message::Rpc(call) => return self.rpc(call),
            Message::Window(Some(id)) => {
                return iced::window::run(id, platform::handle).map(Message::NativeHandle);
            }
            Message::NativeHandle(handle) => {
                match platform::Desktop::new(handle, self.preferences.shortcut) {
                    Ok(desktop) => self.desktop = Some(desktop),
                    Err(_) => self.error = Some("窗口集成初始化失败。".into()),
                }
                if let (Some(desktop), Some(bounds)) = (&self.desktop, self.preferences.window) {
                    desktop.restore_bounds(bounds);
                }
                let topmost = self.preferences.topmost && !lazybuddy_host_control::isolated();
                let level = iced::window::latest().and_then(move |id| {
                    iced::window::set_level(
                        id,
                        if topmost {
                            iced::window::Level::AlwaysOnTop
                        } else {
                            iced::window::Level::Normal
                        },
                    )
                });
                return Task::batch([
                    level,
                    if !self.hidden {
                        self.resume()
                    } else {
                        Task::none()
                    },
                ]);
            }
            Message::ShowWindow => {
                if let Some(desktop) = &self.desktop {
                    desktop.show();
                }
            }
            Message::HideWindow => {
                if let Some(desktop) = &self.desktop {
                    desktop.hide();
                }
            }
            Message::ToggleWindow if self.capsule => {
                return self.update(Message::Expand);
            }
            Message::ToggleWindow => {
                if let Some(desktop) = &self.desktop {
                    desktop.toggle();
                }
            }
            Message::Visibility(visible) if self.hidden == visible => {
                self.hidden = !visible;
                if self.hidden {
                    return self.suspend_content();
                }
                if !self.capsule && self.video_receiver.is_none() {
                    return self.resume();
                }
            }
            Message::Suspended(generation, false) if generation == self.play_generation => {
                self.stop();
                self.error = Some("播放器暂停失败，已停止播放。".into());
            }
            Message::Quit if !self.quitting => {
                self.quitting = true;
                if let Some(desktop) = &self.desktop {
                    desktop.hide();
                }
                self.stop();
                let closing = std::mem::take(&mut self.closing_sessions);
                return Task::perform(
                    async move {
                        futures::future::join_all(closing).await;
                    },
                    |_| Message::Exit,
                );
            }
            Message::Exit => return iced::exit(),
            Message::Resized(size) => {
                self.width = size.width;
                if let Some(desktop) = &mut self.desktop {
                    desktop.clip();
                }
            }
            Message::Geometry => {
                if let Some(desktop) = &mut self.desktop {
                    desktop.clip();
                }
            }
            Message::SaveGeometry if !self.capsule => {
                self.preferences.window =
                    self.desktop.as_ref().and_then(|desktop| desktop.bounds());
                return self.save();
            }
            Message::OpenLink(url) => {
                if url.starts_with("https://") || url.starts_with("http://") {
                    return Task::perform(
                        background(move || {
                            open::that_detached(url).map_err(|_| "链接打开失败。".into())
                        }),
                        Message::Saved,
                    );
                }
            }
            Message::Followed(value) => {
                if let Some(root) = &self.selected {
                    self.pages.get_mut(&root.id).unwrap().followed_only = value;
                }
            }
            Message::Category(value) => {
                if let Some(root) = &self.selected {
                    self.pages.get_mut(&root.id).unwrap().category =
                        (value != "全部分类").then_some(value);
                }
            }
            Message::NewsPreferences(preferences) if !self.busy => {
                return self.request("configure".into(), json!({"preferences":preferences}));
            }
            Message::Refresh | Message::NewsTick
                if self.section == Section::Content && !self.busy =>
            {
                if let Some(Page::NewsGrid { fetch_method, .. }) = self.template() {
                    let method = fetch_method.clone();
                    let page = self
                        .pages
                        .get_mut(&self.selected.as_ref().unwrap().id)
                        .unwrap();
                    let sources = page.begin_news(page.visible_sources());
                    if !sources.is_empty() {
                        return self.request(method, json!({"sources":sources}));
                    }
                }
            }
            Message::Play(series, episode)
                if self.can_interact() && self.section == Section::Content =>
            {
                self.hold.cancel();
                use ring::rand::SecureRandom;
                let mut random = [0u8; 16];
                if ring::rand::SystemRandom::new().fill(&mut random).is_err() {
                    self.error = Some("无法创建播放连接。".into());
                    return Task::none();
                }
                let pipe = format!(
                    r"\\.\pipe\lazybuddy-media-{}",
                    random
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>()
                );
                self.video_receiver = None;
                self.frame = None;
                self.play_generation += 1;
                if let Some(root) = &self.selected {
                    let page = self.pages.get_mut(&root.id).unwrap();
                    page.controls_menu = false;
                    page.pipe = pipe.clone();
                    page.player_visible = true;
                    page.play_intent = true;
                    page.seek_preview = None;
                    let episodes = if page.media["series"] == series {
                        page.media["episodes"].clone()
                    } else {
                        Value::Null
                    };
                    let position =
                        if page.media["series"] == series && page.media["episode"] == episode {
                            page.media["resume_position"]
                                .as_f64()
                                .or_else(|| page.media["position"].as_f64())
                                .unwrap_or(0.)
                        } else if page.history[&series]["episode"] == episode {
                            page.history[&series]["time"].as_f64().unwrap_or(0.)
                        } else {
                            0.
                        };
                    page.media = json!({"series":series,"episode":episode,"episodes":episodes,"loading":true,"playing":false,"eos":false,"resume_position":position});
                    let preferences = self.preferences.pages.entry(root.id.clone()).or_default();
                    let args = json!({"series":series,"episode":episode,"quality":preferences.quality,"pipe":pipe,"position":position,"speed":preferences.speed,"volume":preferences.volume,"mute":preferences.mute});
                    return self.request("media.open".into(), args);
                }
            }
            Message::Player(method, args)
                if (self.can_interact() || method == "media.boost")
                    && self.section == Section::Content =>
            {
                if matches!(method, "media.pause" | "media.close" | "media.speed") {
                    self.hold.cancel();
                }
                if let Some(root) = &self.selected {
                    let page = self.pages.get_mut(&root.id).unwrap();
                    page.controls_menu = false;
                    if method == "media.close" {
                        page.player_visible = false;
                        page.play_intent = false;
                        self.frame = None;
                        self.video_receiver = None;
                        self.play_generation += 1;
                    }
                    if method == "media.pause" {
                        page.play_intent = false;
                    }
                    if method == "media.play" {
                        page.play_intent = true;
                        if page.media["loaded"] != true && page.media["loading"] != true {
                            if let (Some(series), Some(episode)) = (
                                page.media["series"].as_str(),
                                page.media["episode"].as_u64(),
                            ) {
                                let series = series.to_owned();
                                return self.update(Message::Play(series, episode as u32));
                            }
                        }
                    }
                    let preferences = self.preferences.pages.entry(root.id.clone()).or_default();
                    let save = match method {
                        "media.volume" => args["value"]
                            .as_f64()
                            .filter(|value| (0.0..=1.0).contains(value))
                            .map(|value| preferences.volume = value)
                            .is_some(),
                        "media.speed" => args["value"]
                            .as_f64()
                            .filter(|value| (0.25..=4.0).contains(value))
                            .map(|value| preferences.speed = value)
                            .is_some(),
                        "media.mute" => args["value"]
                            .as_bool()
                            .map(|value| preferences.mute = value)
                            .is_some(),
                        _ => false,
                    };
                    let saved = if save { self.save() } else { Task::none() };
                    let restore = if method == "media.close" {
                        self.restore_scroll()
                    } else {
                        Task::none()
                    };
                    return Task::batch([saved, restore, self.request(method.into(), args)]);
                }
            }
            Message::VolumePreview(value) => {
                if let Some(root) = &self.selected {
                    self.preferences
                        .pages
                        .entry(root.id.clone())
                        .or_default()
                        .volume = value;
                }
            }
            Message::VolumeCommit => {
                if let Some(root) = &self.selected {
                    let value = self
                        .preferences
                        .pages
                        .entry(root.id.clone())
                        .or_default()
                        .volume;
                    return self.update(Message::Player("media.volume", json!({"value":value})));
                }
            }
            Message::Quality(value) if self.can_interact() => {
                if let Some(root) = &self.selected {
                    self.pages.get_mut(&root.id).unwrap().controls_menu = false;
                    let page = &self.pages[&root.id];
                    if let (Some(series), Some(episode)) = (
                        page.media["series"].as_str(),
                        page.media["episode"].as_u64(),
                    ) {
                        let (series, intent) = (series.to_owned(), page.play_intent);
                        self.preferences
                            .pages
                            .entry(root.id.clone())
                            .or_default()
                            .quality = value;
                        let save = self.save();
                        let play = self.update(Message::Play(series, episode as u32));
                        if let Some(root) = &self.selected {
                            self.pages.get_mut(&root.id).unwrap().play_intent = intent;
                        }
                        return Task::batch([save, play]);
                    }
                }
            }
            Message::SeekPreview(position) => {
                if let Some(root) = &self.selected {
                    self.pages.get_mut(&root.id).unwrap().seek_preview = Some(position);
                }
            }
            Message::SeekCommit if self.can_interact() => {
                if let Some(root) = &self.selected
                    && let Some(position) =
                        self.pages.get_mut(&root.id).unwrap().seek_preview.take()
                {
                    return self.request("media.seek".into(), json!({"value":position}));
                }
            }
            Message::VideoTick if !self.busy && self.section == Section::Content => {
                return self.request("media.status".into(), json!({}));
            }
            Message::Frame(id, generation, frame)
                if self.selected.as_ref().is_some_and(|root| root.id == id)
                    && generation == self.play_generation
                    && self.section == Section::Content
                    && !self.hidden
                    && !self.capsule =>
            {
                match frame {
                    Ok(frame) => self.frame = Some(frame),
                    Err(error) => {
                        self.stop();
                        self.error = Some(error);
                    }
                }
            }
            Message::Select(root) => {
                if self.selected.as_ref() == Some(&root) {
                    return Task::none();
                }
                self.stop();
                let default = self
                    .preferences
                    .pages
                    .get(&root.id)
                    .is_some_and(|page| page.default_favorites);
                let page = self
                    .pages
                    .entry(root.id.clone())
                    .or_insert_with(|| PageState {
                        favorites_tab: default,
                        ..Default::default()
                    });
                self.section.select_page(page);
                self.selected = Some(root);
                self.preferences.root = self.selected.as_ref().map(|root| root.id.clone());
                self.error = None;
                return Task::batch([self.save(), self.resume()]);
            }
            Message::Navigate(section) => {
                if self.section == section {
                    return Task::none();
                }
                self.stop();
                if let Section::Settings(Settings::Page) = section
                    && let Some(root) = &self.selected
                {
                    self.pages.get_mut(&root.id).unwrap().last_settings = Some(Settings::Page);
                }
                self.section = section;
                self.error = None;
                if section == Section::Modules && self.catalog.is_none() {
                    return self.load_catalog();
                }
                return self.resume();
            }
            Message::Fetch(source) if !self.busy && self.section == Section::Content => {
                if let Some(Page::NewsGrid { fetch_method, .. }) = self.template() {
                    let method = fetch_method.clone();
                    let sources = self
                        .pages
                        .get_mut(&self.selected.as_ref().unwrap().id)
                        .unwrap()
                        .begin_news(vec![source]);
                    return self.request(method, json!({"sources":sources}));
                }
            }
            Message::Query(query) => {
                if let Some(root) = &self.selected {
                    self.pages.get_mut(&root.id).unwrap().query = query;
                }
            }
            Message::Search | Message::More
                if self.can_interact() && self.section == Section::Content =>
            {
                if let Some(Page::MediaLibrary { search_method, .. }) = self.template() {
                    let method = search_method.clone();
                    let page = self.page().unwrap();
                    let next =
                        if matches!(message, Message::More) && page.query == page.result_query {
                            page.page + 1
                        } else {
                            1
                        };
                    return self.request(method, json!({"query":page.query,"page":next}));
                }
            }
            Message::Favorites(tab) if self.section == Section::Content => {
                if let Some(root) = &self.selected {
                    self.pages.get_mut(&root.id).unwrap().favorites_tab = tab;
                    let page = self.pages.get_mut(&root.id).unwrap();
                    page.cover_pending.clear();
                    page.cover_generation += 1;
                    if tab
                        && self.can_interact()
                        && self
                            .page()
                            .is_some_and(|page| page.account["operation"] != "pending")
                        && let Some(Page::MediaLibrary {
                            favorites_method, ..
                        }) = self.template()
                    {
                        let method = favorites_method.clone();
                        return Task::batch([
                            self.restore_scroll(),
                            self.request(method, json!({})),
                        ]);
                    }
                }
                return self.restore_scroll();
            }
            Message::Account(method) => {
                if matches!(method, "account.logout" | "account.login") {
                    self.hold.cancel();
                    self.frame = None;
                    self.video_receiver = None;
                    self.play_generation += 1;
                    if let Some(root) = &self.selected {
                        let page = self.pages.get_mut(&root.id).unwrap();
                        page.player_visible = false;
                        page.play_intent = false;
                    }
                }
                if matches!(self.template(), Some(Page::MediaLibrary { .. }))
                    && matches!(
                        self.section,
                        Section::Content | Section::Settings(Settings::Page)
                    )
                {
                    return self.request(method.into(), json!({}));
                }
            }
            Message::Poll if !self.busy => return self.request("account.status".into(), json!({})),
            Message::Scroll(id, generation, viewport)
                if id == self.list_id()
                    && generation == self.play_generation
                    && self.section == Section::Content
                    && !self.hidden
                    && !self.capsule =>
            {
                if let Some(root) = &self.selected {
                    self.pages
                        .get_mut(&root.id)
                        .unwrap()
                        .scroll
                        .insert(id, viewport.absolute_offset());
                }
            }
            Message::Result(id, generation, method, result)
                if self.activation.accepts(&id, generation) =>
            {
                return self.accept_result(id, method, result);
            }
            Message::Dark(dark) => {
                self.preferences.dark = dark;
                return self.save();
            }
            Message::DefaultFavorites(value) => {
                if let Some(root) = &self.selected {
                    self.preferences
                        .pages
                        .entry(root.id.clone())
                        .or_default()
                        .default_favorites = value;
                    return self.save();
                }
            }
            Message::Uninstall(id) if !self.manager_busy => {
                self.stop();
                self.manager_busy = true;
                let manager = Manager::new(self.manager.root().to_owned());
                let closing = self.closing_sessions.clone();
                return Task::perform(
                    async move {
                        for completion in closing {
                            completion.await;
                        }
                        background(move || {
                            manager.uninstall(&id).map_err(|_| {
                                "功能列表已重新核对；尚有文件未清理时，请稍后重试。".into()
                            })
                        })
                        .await
                    },
                    Message::Removed,
                );
            }
            Message::Removed(result) => {
                self.manager_busy = false;
                self.reload();
                match result {
                    Ok(()) => {
                        self.pages.retain(|id, _| self.installed.contains_key(id));
                        if self
                            .selected
                            .as_ref()
                            .is_some_and(|root| !self.roots.contains(root))
                        {
                            self.selected = None;
                        }
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::Reload if !self.manager_busy => {
                self.reload();
                return self.load_catalog();
            }
            Message::CatalogLoaded(result) => {
                self.manager_busy = false;
                match result {
                    Ok(catalog) => {
                        self.catalog = Some(catalog);
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::Install(id) if !self.manager_busy => return self.install_module(id),
            Message::Saved(Err(error)) => self.error = Some(error),
            _ => {}
        }
        self.next_cover()
    }
    fn theme(&self) -> iced::Theme {
        if self.preferences.dark {
            iced::Theme::Dark
        } else {
            iced::Theme::Light
        }
    }
    fn view(&self) -> Element<'_, Message> {
        view::view(self)
    }
}
impl Drop for App {
    fn drop(&mut self) {
        self.stop();
        futures::executor::block_on(futures::future::join_all(std::mem::take(
            &mut self.closing_sessions,
        )));
    }
}
fn main() -> iced::Result {
    lazybuddy_host_control::data_dir().map_err(iced::Error::ExecutorCreationFailed)?;
    let command = if std::env::args().any(|arg| arg == "--background") {
        "status"
    } else {
        "window.show"
    };
    if lazybuddy_host_control::call_sync(&lazybuddy_host_control::Request {
        version: 1,
        command: command.into(),
        args: json!({}),
    })
    .is_ok_and(|reply| reply["ok"] == true)
    {
        return Ok(());
    }
    let identity: Vec<u16> = "LazyBuddy.Desktop".encode_utf16().chain(Some(0)).collect();
    unsafe {
        let _ = windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(
            identity.as_ptr(),
        );
    }
    iced::application(App::new, App::update, App::view)
        .title("LazyBuddy")
        .subscription(App::subscription)
        .theme(App::theme)
        .style(|_, theme| iced::theme::Style {
            background_color: iced::Color::TRANSPARENT,
            text_color: theme.palette().text,
        })
        .default_font(iced::Font::with_name("Microsoft YaHei UI"))
        .window(iced::window::Settings {
            visible: !std::env::args().any(|arg| arg == "--background"),
            size: iced::Size::new(780., 560.),
            exit_on_close_request: false,
            decorations: false,
            transparent: true,
            min_size: Some(iced::Size::new(400., 240.)),
            icon: ::image::load_from_memory(include_bytes!("../../../assets/lazybuddy.ico"))
                .ok()
                .and_then(|image| {
                    let image = image.to_rgba8();
                    iced::window::icon::from_rgba(
                        image.as_raw().clone(),
                        image.width(),
                        image.height(),
                    )
                    .ok()
                }),
            ..Default::default()
        })
        .run()
}
