//! Merge replies only after the caller checked the current activation.
use super::*;

impl App {
    pub(super) fn accept_result(
        &mut self,
        id: String,
        method: String,
        result: Result<Value, String>,
    ) -> Task<Message> {
        self.busy = false;
        self.current_method.clear();
        if method.starts_with("media.")
            && self
                .queued_requests
                .iter()
                .any(|(method, _)| matches!(method.as_str(), "media.open" | "media.close"))
        {
            return self.finish_reply();
        }
        if method == "cover.read" {
            if let Ok(reply) = result {
                let data = &reply["data"];
                if reply["ok"] == true
                    && reply["id"] == format!("cover:{}", self.pages[&id].cover_generation)
                {
                    use base64::Engine;
                    if let (Some(series), Some(width), Some(height), Some(encoded)) = (
                        data["series"].as_str(),
                        data["width"].as_u64(),
                        data["height"].as_u64(),
                        data["rgba"].as_str(),
                    ) && width > 0
                        && width <= 240
                        && height > 0
                        && height <= 360
                        && encoded.len() <= 240 * 360 * 4 * 4 / 3 + 4
                        && let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded)
                        && bytes.len() == (width * height * 4) as usize
                    {
                        self.pages.get_mut(&id).unwrap().covers.insert(
                            series.into(),
                            iced::widget::image::Handle::from_rgba(
                                width as u32,
                                height as u32,
                                bytes,
                            ),
                        );
                    }
                }
            }
            return self.finish_reply();
        }
        let data = match result {
            Ok(reply) if reply["ok"] == true => reply["data"].clone(),
            reply => {
                self.error = Some(
                    match reply
                        .as_ref()
                        .ok()
                        .and_then(|reply| reply["error"]["code"].as_str())
                    {
                        Some("app_access_denied" | "media_access_denied") => {
                            "当前账号无权播放该内容。"
                        }
                        Some(
                            "saved_credentials_missing"
                            | "saved_session_verification_failed"
                            | "login_required",
                        ) => "请返回账号设置重新确认登录状态。",
                        Some("media_network_error" | "app_source_network_error") => {
                            "网络连接失败，请稍后重试。"
                        }
                        Some("progress_save_failed") => "观看进度未能保存，原记录已保留。",
                        _ => "功能请求失败，请重试。",
                    }
                    .into(),
                );
                if method.starts_with("media.") {
                    let page = self.pages.get_mut(&id).unwrap();
                    page.play_intent = false;
                    page.media["loading"] = json!(false);
                }
                if method == "favorites.set" {
                    self.pages.get_mut(&id).unwrap().favorite_override = None;
                }
                return self.finish_reply();
            }
        };
        let page = self.pages.get_mut(&id).unwrap();
        if method.starts_with("media.") {
            if data["loaded"] == true {
                page.media["resume_position"] = data["position"].clone();
            }
            if let Some(target) = page.media.as_object_mut()
                && let Some(updates) = data.as_object()
            {
                target.extend(updates.clone());
            }
            if page.media["error"].is_string() {
                page.play_intent = false;
                page.media["loading"] = json!(false);
                self.video_receiver = None;
                self.frame = None;
                self.play_generation += 1;
                self.error = Some(
                    if matches!(
                        page.media["error"].as_str(),
                        Some("app_access_denied" | "media_access_denied")
                    ) {
                        "当前账号无权播放该内容。"
                    } else {
                        "视频载入失败，可重试或选择其他集。"
                    }
                    .into(),
                );
                return Task::none();
            }
            if self.video_receiver.is_none()
                && data["producer_pid"].is_u64()
                && data["loading"] != true
            {
                let Some(pid) = data["producer_pid"]
                    .as_u64()
                    .and_then(|pid| u32::try_from(pid).ok())
                else {
                    self.error = Some("播放进程未就绪。".into());
                    return Task::none();
                };
                let (receiver, frames) =
                    lazybuddy_media_ipc::receiver::Receiver::start(page.pipe.clone(), pid);
                self.video_receiver = Some(receiver);
                let generation = self.play_generation;
                let stream = Task::run(frames, move |frame| {
                    Message::Frame(id.clone(), generation, frame)
                });
                let play = page.play_intent;
                return Task::batch([
                    stream,
                    if play {
                        self.continue_reply_request("media.play".into(), json!({}))
                    } else {
                        self.finish_reply()
                    },
                ]);
            }
            if page.media["eos"] == true && page.play_intent && self.queued_requests.is_empty() {
                page.play_intent = false;
                let episode = page.media["episode"].as_u64().unwrap_or(0);
                let next = page.media["episodes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                    .find(|&number| number > episode);
                if let (Some(next), Some(series)) = (next, page.media["series"].as_str()) {
                    let series = series.to_owned();
                    return self.update(Message::Play(series, next as u32));
                }
            }
            return self.finish_reply();
        }
        match self
            .modules
            .iter()
            .find(|module| module.id == id)
            .and_then(|module| module.page.as_ref())
        {
            Some(Page::NewsGrid { catalog_method, .. }) if method == *catalog_method => {
                page.catalog = data.as_array().cloned().unwrap_or_default();
                return self.continue_reply_request(
                    if self.section == Section::Content {
                        "state"
                    } else {
                        "preferences"
                    }
                    .into(),
                    json!({}),
                );
            }
            Some(Page::NewsGrid { fetch_method, .. })
                if matches!(method.as_str(), "state" | "preferences" | "configure") =>
            {
                page.news_preferences = data["preferences"].clone();
                if method == "state" && self.section == Section::Content {
                    for card in data["cache"]
                        .as_object()
                        .into_iter()
                        .flat_map(|cache| cache.values())
                    {
                        if let Some(old) = page.cards.iter_mut().find(|old| old["id"] == card["id"])
                        {
                            if card["updated_ms"].as_u64() > old["updated_ms"].as_u64() {
                                *old = card.clone();
                            }
                        } else {
                            page.cards.push(card.clone());
                        }
                    }
                    if !page.news_loaded {
                        page.followed_only = page.news_preferences["default_followed"] == true;
                    }
                    let refresh = !page.news_loaded
                        && page.news_preferences["auto_refresh"] == true
                        && page.cards.is_empty();
                    page.news_loaded = true;
                    if refresh {
                        return self.update(Message::Refresh);
                    }
                    let sources = page.resume_news();
                    if !sources.is_empty() {
                        return self.continue_reply_request(
                            fetch_method.clone(),
                            json!({"sources":sources}),
                        );
                    }
                }
            }
            Some(Page::NewsGrid { .. }) => {
                for card in data.as_array().into_iter().flatten() {
                    if let Some(index) = page.cards.iter().position(|old| old["id"] == card["id"]) {
                        if card["ok"] == false {
                            page.cards[index]["ok"] = json!(false);
                        } else {
                            page.cards[index] = card.clone();
                        }
                    } else {
                        page.cards.push(card.clone());
                    }
                }
                let sources = page.next_news_batch();
                if !sources.is_empty() {
                    let method = method.clone();
                    return self.continue_reply_request(method, json!({"sources":sources}));
                }
            }
            Some(Page::MediaLibrary { search_method, .. }) if method == *search_method => {
                if self.request_query.as_deref() != Some(page.query.as_str()) {
                    return self.finish_reply();
                }
                let rows = data["items"].as_array().cloned().unwrap_or_default();
                if data["page"] == 1 {
                    page.cards = rows;
                } else {
                    page.cards.extend(rows);
                }
                page.page = data["page"].as_u64().unwrap_or(1);
                page.has_more = data["has_more"] == true;
                page.result_query = page.query.clone();
                page.cover_pending = page
                    .cards
                    .iter()
                    .filter(|row| {
                        row["has_cover"] == true
                            && row["id"]
                                .as_str()
                                .is_some_and(|id| !page.covers.contains_key(id))
                    })
                    .filter_map(|row| row["id"].as_str().map(str::to_owned))
                    .collect();
            }
            Some(Page::MediaLibrary { .. }) => {
                if matches!(method.as_str(), "progress.read" | "progress.sync")
                    || data["operation_method"] == "progress.sync"
                {
                    page.history = data["history"].clone();
                    page.account["operation"] = data["operation"].clone();
                    page.account["sync_error"] = data["sync_error"].clone();
                    page.account["sync_state"] = data["sync_state"].clone();
                    if page.media["loaded"] != true
                        && let Some(series) = page.media["series"].as_str()
                    {
                        let history = &page.history[series];
                        let position = if history["episode"] == page.media["episode"] {
                            history["time"].as_f64().unwrap_or(0.)
                        } else {
                            0.
                        };
                        page.media["resume_position"] = json!(position);
                    }
                    if data["sync_error"].is_string() {
                        self.error = Some("云端进度暂未同步，保留本地观看记录。".into());
                    }
                    if data["operation"] != "pending"
                        && self.section == Section::Content
                        && page.account["needs_favorites"] == true
                    {
                        page.account["needs_favorites"] = json!(false);
                        return self.continue_reply_request("favorites.read".into(), json!({}));
                    }
                    return self.finish_reply();
                }
                if let Some(favorites) = data["favorites"].as_array() {
                    page.favorites = favorites.clone();
                    if page.favorites_tab {
                        page.cover_pending = favorites
                            .iter()
                            .filter(|row| {
                                row["has_cover"] == true
                                    && row["id"]
                                        .as_str()
                                        .is_some_and(|id| !page.covers.contains_key(id))
                            })
                            .filter_map(|row| row["id"].as_str().map(str::to_owned))
                            .collect();
                    }
                }
                if data["state"] != "signed_in" {
                    page.favorites.clear();
                    page.covers.clear();
                    page.cover_pending.clear();
                    page.history = Value::Null;
                    page.media = Value::Null;
                    page.player_visible = false;
                    page.play_intent = false;
                    page.favorite_override = None;
                    self.frame = None;
                    self.video_receiver = None;
                    self.play_generation += 1;
                }
                if page.account["qr_png"] != data["qr_png"] {
                    use base64::Engine;
                    let bytes = data["qr_png"]
                        .as_str()
                        .filter(|encoded| encoded.len() <= 1_400_000)
                        .and_then(|encoded| {
                            base64::engine::general_purpose::STANDARD
                                .decode(encoded)
                                .ok()
                        })
                        .unwrap_or_default();
                    let reader = ::image::ImageReader::new(std::io::Cursor::new(bytes))
                        .with_guessed_format();
                    page.login_qr = reader
                        .ok()
                        .and_then(|mut reader| {
                            let mut limits = ::image::Limits::default();
                            limits.max_image_width = Some(2048);
                            limits.max_image_height = Some(2048);
                            limits.max_alloc = Some(32 * 1024 * 1024);
                            reader.limits(limits);
                            reader.decode().ok()
                        })
                        .map(|pixels| {
                            let pixels = pixels.to_rgba8();
                            iced::widget::image::Handle::from_rgba(
                                pixels.width(),
                                pixels.height(),
                                pixels.into_raw(),
                            )
                        });
                }
                let signed_in = data["state"] == "signed_in";
                let newly_signed_in = page.account["state"] != "signed_in";
                if data["operation"] != "pending" {
                    page.favorite_override = None;
                    if data["favorites_error"].is_string() {
                        self.error = Some("收藏同步失败，保留上次确认结果。".into());
                    }
                }
                page.account = data;
                if signed_in
                    && (method == "account.restore" || newly_signed_in)
                    && self.section == Section::Content
                {
                    page.account["needs_favorites"] = json!(true);
                    return self.continue_reply_request("progress.sync".into(), json!({}));
                }
            }
            _ => {}
        }
        self.finish_reply()
    }
    pub(super) fn finish_reply(&mut self) -> Task<Message> {
        if let Some((method, args)) = self.queued_requests.pop_front() {
            self.request(method, args)
        } else {
            self.next_cover()
        }
    }
    fn continue_reply_request(&mut self, method: String, args: Value) -> Task<Message> {
        self.queued_requests.push_back((method, args));
        self.finish_reply()
    }
}
