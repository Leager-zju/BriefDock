//! Local CLI commands enter exactly the same update paths as Iced controls.
use super::*;
use lazybuddy_host_control::{Reply, Request};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct Incoming {
    pub request: Request,
    reply: Arc<Mutex<Option<Reply>>>,
}
impl Incoming {
    fn reply(&self, value: Value) {
        if let Some(reply) = self.reply.lock().unwrap().take() {
            let _ = reply.send(value);
        }
    }
}

pub fn start() -> std::io::Result<()> {
    lazybuddy_host_control::serve(|request, reply| {
        platform::emit(Message::Rpc(Incoming {
            request,
            reply: Arc::new(Mutex::new(Some(reply))),
        }))
    })
}

impl App {
    pub(super) fn save_capture(
        call: Incoming,
        screenshot: iced::window::Screenshot,
    ) -> Task<Message> {
        Task::perform(
            background(move || {
                use image::ImageEncoder;
                let path = call.request.args["path"].as_str().unwrap();
                let result = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .and_then(|file| {
                        image::codecs::png::PngEncoder::new(file)
                            .write_image(
                                &screenshot.rgba,
                                screenshot.size.width,
                                screenshot.size.height,
                                image::ExtendedColorType::Rgba8,
                            )
                            .map_err(std::io::Error::other)
                    });
                call.reply(match result {Ok(())=>json!({"ok":true,"data":{"path":path,"width":screenshot.size.width,"height":screenshot.size.height,"backend":"iced-renderer"}}),Err(_)=>json!({"ok":false,"error":{"code":"capture_failed"}})});
                Ok(())
            }),
            Message::Saved,
        )
    }
    pub(super) fn rpc(&mut self, call: Incoming) -> Task<Message> {
        let request = &call.request;
        if request.command == "window.capture" {
            if self.hidden
                || !request.args["path"].as_str().is_some_and(|path| {
                    std::path::Path::new(path).is_absolute()
                        && std::path::Path::new(path)
                            .extension()
                            .is_some_and(|extension| extension == "png")
                })
            {
                call.reply(json!({"ok":false,"error":{"code":"invalid_capture_or_hidden"}}));
                return Task::none();
            }
            return iced::window::latest().and_then(move |id| {
                let call = call.clone();
                iced::window::screenshot(id).map(move |shot| Message::Capture(call.clone(), shot))
            });
        }
        if request.command == "status" {
            let page = self.page();
            let media = page.map(|page| {
                let mut value = json!({});
                for key in [
                    "series",
                    "episode",
                    "episodes",
                    "playing",
                    "eos",
                    "position",
                    "duration",
                    "speed",
                    "actual_speed",
                    "boosted",
                    "loaded",
                    "loading",
                    "error",
                    "diagnostic",
                    "width",
                    "height",
                    "decoded_width",
                    "decoded_height",
                ] {
                    value[key] = page.media[key].clone();
                }
                value
            });
            call.reply(json!({"ok":true,"data":{"pid":std::process::id(),"exe":std::env::current_exe().ok(),"capsule":self.capsule,"window":self.desktop.as_ref().and_then(|desktop|desktop.bounds()),"root":self.selected.as_ref().map(|root|&root.id),"section":format!("{:?}",self.section),"hidden":self.hidden,"busy":self.busy,"method":self.current_method,"error":self.error,
                "modules":self.installed,"manager_busy":self.manager_busy,"available_modules":self.catalog.as_ref().map(|catalog|catalog.packages.iter().filter(|package|package.kind==Kind::Feature).map(|package|package.id.clone()).collect::<Vec<_>>()),"service_active":self.session.is_some(),"news_sources":page.map(|page|page.catalog.len()),"results":page.map(|page|page.cards.len()),"query":page.map(|page|&page.query),"account_operation":page.map(|page|page.account["operation"].clone()),"favorite_pending":page.is_some_and(|page|page.favorite_override.is_some()),"favorite":page.is_some_and(|page|page.favorites.iter().any(|row|row["id"]==page.media["series"])),"account_state":page.map(|page|page.account["state"].clone()),"qr_present":page.is_some_and(|page|page.login_qr.is_some()),"media":media}}));
            return Task::none();
        }
        let args = &request.args;
        if request.command == "player.favorite"
            && (!self.can_interact()
                || self.page().is_some_and(|page| {
                    page.favorite_override.is_some() || page.account["operation"] == "pending"
                }))
        {
            call.reply(json!({"ok":false,"error":{"code":"account_busy"}}));
            return Task::none();
        }
        if matches!(
            request.command.as_str(),
            "modules.install" | "modules.uninstall" | "modules.refresh"
        ) && (self.hidden
            || self.capsule
            || self.section != Section::Modules
            || self.manager_busy)
        {
            call.reply(json!({"ok":false,"error":{"code":"module_manager_unavailable"}}));
            return Task::none();
        }
        let needs_media = request.command.starts_with("player.")
            || request.command.starts_with("account.")
            || matches!(request.command.as_str(), "search" | "favorites");
        if needs_media && !matches!(self.template(), Some(Page::MediaLibrary { .. })) {
            call.reply(json!({"ok":false,"error":{"code":"unsupported_page"}}));
            return Task::none();
        }
        if (self.hidden || self.capsule)
            && matches!(
                request.command.as_str(),
                "search"
                    | "favorites"
                    | "news.refresh"
                    | "account.login"
                    | "account.cancel"
                    | "account.logout"
                    | "player.open"
                    | "player.play"
                    | "player.pause"
                    | "player.close"
                    | "player.seek"
                    | "player.speed"
                    | "player.volume"
                    | "player.mute"
                    | "player.boost"
                    | "player.favorite"
            )
        {
            call.reply(json!({"ok":false,"error":{"code":"view_hidden"}}));
            return Task::none();
        }
        let message = match request.command.as_str() {
            "page.select" => args["id"]
                .as_str()
                .and_then(|id| self.roots.iter().find(|root| root.id == id))
                .cloned()
                .map(Message::Select),
            "settings.open" => match args["category"].as_str() {
                Some("page") => Some(Settings::Page),
                Some("display") => Some(Settings::Display),
                Some("system") => Some(Settings::System),
                Some("about") => Some(Settings::About),
                _ => None,
            }
            .map(|category| Message::Navigate(Section::Settings(category))),
            "settings.back" => Some(Message::Navigate(Section::Content)),
            "modules.open" => Some(Message::Navigate(Section::Modules)),
            "modules.refresh" => Some(Message::Reload),
            "modules.install" => args["id"]
                .as_str()
                .filter(|id| {
                    self.catalog.as_ref().is_some_and(|catalog| {
                        catalog
                            .packages
                            .iter()
                            .any(|package| package.id == *id && package.kind == Kind::Feature)
                    })
                })
                .map(|id| Message::Install(id.into())),
            "modules.uninstall" => args["id"]
                .as_str()
                .filter(|id| self.modules.iter().any(|module| module.id == *id))
                .map(|id| Message::Uninstall(id.into())),
            "news.refresh" => Some(Message::Refresh),
            "search" => args["query"]
                .as_str()
                .filter(|query| query.chars().count() <= 80)
                .map(|query| {
                    if let Some(root) = &self.selected {
                        self.pages.get_mut(&root.id).unwrap().query = query.into();
                    }
                    Message::Search
                }),
            "favorites" => args["value"].as_bool().map(Message::Favorites),
            "account.login" => Some(Message::Account("account.login")),
            "account.cancel" => Some(Message::Account("account.cancel")),
            "account.logout" => Some(Message::Account("account.logout")),
            "player.open" => args["series"]
                .as_str()
                .zip(args["episode"].as_u64())
                .filter(|(series, episode)| {
                    series.len() <= 32
                        && series.bytes().all(|byte| byte.is_ascii_digit())
                        && (1..=9999).contains(episode)
                })
                .map(|(series, episode)| Message::Play(series.into(), episode as u32)),
            "player.play" => Some(Message::Player("media.play", json!({}))),
            "player.pause" => Some(Message::Player("media.pause", json!({}))),
            "player.close" => Some(Message::Player("media.close", json!({}))),
            "player.seek" => args["value"]
                .as_f64()
                .filter(|value| (0.0..=86400.0).contains(value))
                .map(|value| Message::Player("media.seek", json!({"value":value}))),
            "player.speed" => args["value"]
                .as_f64()
                .filter(|value| (0.25..=4.0).contains(value))
                .map(|value| Message::Player("media.speed", json!({"value":value}))),
            "player.volume" => args["value"]
                .as_f64()
                .filter(|value| (0.0..=1.0).contains(value))
                .map(|value| Message::Player("media.volume", json!({"value":value}))),
            "player.mute" => args["value"]
                .as_bool()
                .map(|value| Message::Player("media.mute", json!({"value":value}))),
            "player.boost" => args["value"]
                .as_bool()
                .map(|value| Message::Player("media.boost", json!({"value":value}))),
            "player.favorite" => args["value"].as_bool().map(Message::Favorite),
            "player.controls" => args["value"].as_bool().map(Message::ControlsHover),
            "window.show" => Some(Message::ShowWindow),
            "window.dock" => match args["edge"].as_str() {
                Some("left") => Some(Message::Dock(Some(platform::Edge::Left))),
                Some("right") => Some(Message::Dock(Some(platform::Edge::Right))),
                _ => None,
            },
            "window.expand" => Some(Message::Expand),
            "window.hide" => Some(Message::HideWindow),
            "window.toggle" => Some(Message::ToggleWindow),
            "app.quit" => Some(Message::Quit),
            _ => None,
        };
        let Some(message) = message else {
            call.reply(json!({"ok":false,"error":{"code":"invalid_command_or_arguments"}}));
            return Task::none();
        };
        call.reply(json!({"ok":true,"accepted":true}));
        self.update(message)
    }
}
