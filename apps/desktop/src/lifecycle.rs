//! Visible-page scheduling, service ownership and scoped subscriptions.
use super::*;

impl App {
    pub(super) fn cancel_shortcut(&mut self) {
        self.shortcut_recording = false;
        if let Some(desktop) = &self.desktop {
            desktop.capture_shortcut(false);
        }
    }
    pub(super) fn suspend_content(&mut self) -> Task<Message> {
        self.hold.cancel();
        self.cancel_shortcut();
        self.queued_requests.clear();
        self.current_method.clear();
        if self.video_receiver.is_some()
            && let Some(session) = &self.session
        {
            self.activation.select(None);
            self.busy = false;
            if let Some(root) = &self.selected {
                let page = self.pages.get_mut(&root.id).unwrap();
                page.play_intent = false;
                page.seek_preview = None;
                page.media["playing"] = json!(false);
            }
            let future = session
                .request(json!({"version":1,"id":"suspend","method":"media.pause","args":{}}));
            let generation = self.play_generation;
            return Task::perform(
                async move { future.await.is_ok_and(|reply| reply["ok"] == true) },
                move |success| Message::Suspended(generation, success),
            );
        }
        self.stop();
        Task::none()
    }
    pub(super) fn reload(&mut self) {
        let previous = self.installed.clone();
        match self.manager.list() {
            Ok(modules) => {
                self.installed = modules
                    .iter()
                    .map(|module| (module.id.clone(), module.version.clone()))
                    .collect();
                self.modules = modules
                    .into_iter()
                    .filter(|module| module.kind == Kind::Feature)
                    .collect();
                self.roots = self
                    .modules
                    .iter()
                    .map(|module| Root {
                        id: module.id.clone(),
                        name: module.name.clone(),
                    })
                    .collect();
                self.pages
                    .retain(|id, _| previous.get(id) == self.installed.get(id));
                self.selected = self
                    .selected
                    .as_ref()
                    .and_then(|selected| self.roots.iter().find(|root| root.id == selected.id))
                    .cloned();
                if let Some(root) = &self.selected {
                    let default = self
                        .preferences
                        .pages
                        .get(&root.id)
                        .is_some_and(|page| page.default_favorites);
                    self.pages
                        .entry(root.id.clone())
                        .or_insert_with(|| PageState {
                            favorites_tab: default,
                            ..Default::default()
                        });
                }
            }
            Err(_) => self.error = Some("功能列表读取失败，请稍后重试。".into()),
        }
    }
    pub(super) fn page(&self) -> Option<&PageState> {
        self.selected
            .as_ref()
            .and_then(|root| self.pages.get(&root.id))
    }
    pub(super) fn template(&self) -> Option<&Page> {
        self.selected
            .as_ref()
            .and_then(|root| self.modules.iter().find(|module| module.id == root.id))
            .and_then(|module| module.page.as_ref())
    }
    pub(super) fn stop(&mut self) {
        self.hold.cancel();
        self.cancel_shortcut();
        if let Some(session) = self.session.take() {
            if self.page().is_some_and(|page| {
                page.media["loaded"] == true
                    || page.media["progress_pending"] == true
                    || page.favorite_override.is_some()
            }) {
                use futures::FutureExt;
                self.closing_sessions
                    .retain(|completion| completion.clone().now_or_never().is_none());
                self.closing_sessions
                    .push(session.shutdown().boxed().shared());
            } else {
                session.stop();
            }
        }
        self.activation.select(None);
        self.busy = false;
        self.current_method.clear();
        self.queued_requests.clear();
        self.play_generation += 1;
        self.video_receiver = None;
        self.frame = None;
        if let Some(root) = &self.selected {
            let page = self.pages.get_mut(&root.id).unwrap();
            page.play_intent = false;
            page.controls_hover = false;
            page.controls_keyboard = false;
            page.controls_menu = false;
            page.favorite_override = None;
            if page.player_visible {
                page.media["loading"] = json!(false);
                page.media["playing"] = json!(false);
                page.media["loaded"] = json!(false);
            }
            page.pause_news();
            page.cover_pending.clear();
            page.cover_generation += 1;
            page.seek_preview = None;
        }
    }
    pub(super) fn request(&mut self, method: String, args: Value) -> Task<Message> {
        if self.hidden || self.capsule || self.quitting {
            return Task::none();
        }
        if self.busy {
            if (self.current_method == "cover.read" && method != "cover.read")
                || matches!(method.as_str(), "account.cancel" | "account.logout")
                || method == "media.boost"
                || self.current_method == "media.boost"
                || (self.current_method == "media.status" && method != "media.status")
                || (self.current_method.starts_with("media.") && method.starts_with("media."))
                || (self
                    .page()
                    .is_some_and(|page| page.media["loading"] == true)
                    && method.starts_with("media."))
            {
                if matches!(method.as_str(), "media.open" | "media.close") {
                    self.queued_requests
                        .retain(|(old, _)| !old.starts_with("media."));
                }
                self.queued_requests.retain(|(old, _)| {
                    old != &method
                        && !(matches!(method.as_str(), "media.play" | "media.pause")
                            && matches!(old.as_str(), "media.play" | "media.pause"))
                });
                self.queued_requests.push_back((method, args));
            }
            return Task::none();
        }
        let Some(root) = self.selected.clone() else {
            return Task::none();
        };
        let generation = self.activation.select(Some(&root.id));
        if self.session.is_none() {
            let launch = match self.manager.launch(&root.id) {
                Ok(launch) => launch,
                Err(_) => {
                    self.error = Some("功能程序不可用，请重新安装此功能。".into());
                    return Task::none();
                }
            };
            self.session = Some(lazybuddy_service_client::Session::with_environment(
                launch.executable,
                launch.arguments,
                launch.environment,
            ));
        }
        let identifier = if method == "cover.read" {
            format!("cover:{}", self.pages[&root.id].cover_generation)
        } else {
            "view".into()
        };
        self.request_query = args["query"].as_str().map(str::to_owned);
        let future = self
            .session
            .as_ref()
            .unwrap()
            .request(json!({"version":1,"id":identifier,"method":method,"args":args}));
        self.busy = true;
        self.current_method = method.clone();
        if !matches!(
            method.as_str(),
            "media.status" | "account.status" | "cover.read"
        ) {
            self.error = None;
        }
        Task::perform(
            async move {
                (
                    root.id,
                    generation,
                    method,
                    future
                        .await
                        .map_err(|_| "功能请求失败，请重新进入后重试。".into()),
                )
            },
            |(id, generation, method, result)| Message::Result(id, generation, method, result),
        )
    }
    pub(super) fn list_id(&self) -> String {
        format!(
            "list:{}:{}",
            self.selected
                .as_ref()
                .map(|root| root.id.as_str())
                .unwrap_or("empty"),
            self.page().is_some_and(|page| page.favorites_tab)
        )
    }
    pub(super) fn restore_scroll(&self) -> Task<Message> {
        let id = self.list_id();
        let offset = self
            .page()
            .and_then(|page| page.scroll.get(&id))
            .copied()
            .unwrap_or_default();
        iced::widget::operation::scroll_to(iced::widget::Id::from(id), offset)
    }
    pub(super) fn resume(&mut self) -> Task<Message> {
        Task::batch([self.restore_scroll(), self.resume_requests()])
    }
    pub(super) fn resume_requests(&mut self) -> Task<Message> {
        match self.section {
            Section::Content | Section::Settings(Settings::Page) => match self.template() {
                Some(Page::NewsGrid { catalog_method, .. })
                    if self.page().is_some_and(|page| page.catalog.is_empty()) =>
                {
                    self.request(catalog_method.clone(), json!({}))
                }
                Some(Page::MediaLibrary { .. }) => {
                    self.request("account.restore".into(), json!({}))
                }
                Some(Page::NewsGrid { .. }) => self.request(
                    if self.section == Section::Content {
                        "state"
                    } else {
                        "preferences"
                    }
                    .into(),
                    json!({}),
                ),
                _ => Task::none(),
            },
            _ => Task::none(),
        }
    }
    pub(super) fn save(&self) -> Task<Message> {
        if !self.preferences_valid {
            return Task::none();
        }
        Task::perform(
            self.preferences_writer.save(self.preferences.clone()),
            Message::Saved,
        )
    }
    pub(super) fn can_interact(&self) -> bool {
        (!self.busy
            || self.current_method == "media.boost"
            || self.current_method == "media.status"
            || self.current_method == "cover.read"
            || (self.current_method.starts_with("media.")
                && self
                    .page()
                    .is_some_and(|page| page.media["loading"] == true)))
            && self.queued_requests.is_empty()
    }
    pub(super) fn next_cover(&mut self) -> Task<Message> {
        if self.section == Section::Content
            && !self.busy
            && self.page().is_some_and(|page| !page.player_visible)
            && let Some(root) = &self.selected
            && let Some(id) = self
                .pages
                .get_mut(&root.id)
                .unwrap()
                .cover_pending
                .pop_front()
        {
            return self.request("cover.read".into(), json!({"series":id}));
        }
        Task::none()
    }
    pub(super) fn subscription(&self) -> Subscription<Message> {
        let global = Subscription::batch([
            iced::event::listen_with(|event, status, _| match event {
                iced::Event::Keyboard(event) => Some(Message::Keyboard(event, status)),
                iced::Event::Window(iced::window::Event::Unfocused) => Some(Message::Blur),
                _ => None,
            }),
            Subscription::run(platform::events),
            iced::window::close_requests().map(|_| Message::Quit),
            iced::window::resize_events().map(|(_, size)| Message::Resized(size)),
        ]);
        if self.hidden || self.capsule || self.quitting {
            return global;
        }
        let login_visible = matches!(
            self.section,
            Section::Content | Section::Settings(Settings::Page)
        );
        let account = if login_visible
            && self.page().is_some_and(|page| {
                matches!(
                    page.account["state"].as_str(),
                    Some("preparing" | "pending" | "scanned" | "verifying")
                ) || page.account["operation"] == "pending"
            }) {
            iced::time::every(std::time::Duration::from_secs(1)).map(|_| Message::Poll)
        } else {
            Subscription::none()
        };
        let player = if self.section == Section::Content
            && self.page().is_some_and(|page| {
                page.player_visible
                    && (page.play_intent
                        || page.media["playing"] == true
                        || page.media["loading"] == true)
                    && page.media["eos"] != true
            }) {
            iced::time::every(std::time::Duration::from_millis(250)).map(|_| Message::VideoTick)
        } else {
            Subscription::none()
        };
        let news = if self.section == Section::Content
            && matches!(self.template(), Some(Page::NewsGrid { .. }))
            && self
                .page()
                .is_some_and(|page| page.news_preferences["auto_refresh"] == true)
        {
            let minutes = self.page().unwrap().news_preferences["refresh_minutes"]
                .as_u64()
                .unwrap_or(5)
                .clamp(1, 60);
            iced::time::every(std::time::Duration::from_secs(minutes * 60))
                .map(|_| Message::NewsTick)
        } else {
            Subscription::none()
        };
        let hold = if self.hold.pending() {
            iced::time::every(std::time::Duration::from_millis(40)).map(|_| Message::HoldTick)
        } else {
            Subscription::none()
        };
        Subscription::batch([account, player, news, hold, global])
    }
}
