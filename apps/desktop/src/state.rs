use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Content,
    Modules,
    Settings(Settings),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settings {
    Page,
    Display,
    System,
    About,
}
impl std::fmt::Display for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Page => "当前功能",
            Self::Display => "显示设置",
            Self::System => "系统设置",
            Self::About => "关于我们",
        })
    }
}

#[derive(Default)]
pub struct PageState {
    pub catalog: Vec<Value>,
    pub cards: Vec<Value>,
    pub query: String,
    pub page: u64,
    pub has_more: bool,
    pub favorites_tab: bool,
    pub favorites: Vec<Value>,
    pub account: Value,
    pub history: Value,
    pub favorite_override: Option<bool>,
    pub login_qr: Option<iced::widget::image::Handle>,
    pub last_settings: Option<Settings>,
    pub scroll: std::collections::BTreeMap<String, iced::widget::scrollable::AbsoluteOffset>,
    pub media: Value,
    pub player_visible: bool,
    pub controls_hover: bool,
    pub controls_keyboard: bool,
    pub controls_menu: bool,
    pub play_intent: bool,
    pub pipe: String,
    pub seek_preview: Option<f64>,
    pub news_preferences: Value,
    pub followed_only: bool,
    pub category: Option<String>,
    pub pending: std::collections::VecDeque<String>,
    pub news_inflight: Vec<String>,
    pub news_interrupted: std::collections::VecDeque<String>,
    pub news_requested_at: u64,
    pub news_loaded: bool,
    pub covers: std::collections::BTreeMap<String, iced::widget::image::Handle>,
    pub cover_pending: std::collections::VecDeque<String>,
    pub cover_generation: u64,
    pub result_query: String,
}

impl PageState {
    pub fn visible_sources(&self) -> Vec<String> {
        self.catalog
            .iter()
            .filter(|source| {
                !self.news_preferences["hidden"]
                    .as_array()
                    .is_some_and(|v| v.contains(&source["id"]))
                    && (!self.followed_only
                        || self.news_preferences["followed"]
                            .as_array()
                            .is_some_and(|v| v.contains(&source["id"])))
                    && self
                        .category
                        .as_ref()
                        .is_none_or(|v| source["category"] == v.as_str())
            })
            .filter_map(|v| v["id"].as_str().map(str::to_owned))
            .collect()
    }
    pub fn next_news_batch(&mut self) -> Vec<String> {
        self.news_inflight = self.pending.drain(..self.pending.len().min(4)).collect();
        self.news_inflight.clone()
    }
    pub fn pause_news(&mut self) {
        self.news_interrupted.extend(self.news_inflight.drain(..));
        self.news_interrupted.append(&mut self.pending);
    }
    pub fn resume_news(&mut self) -> Vec<String> {
        let visible = self.visible_sources();
        self.pending = self
            .news_interrupted
            .drain(..)
            .filter(|id| {
                visible.contains(id)
                    && !self.cards.iter().any(|card| {
                        card["id"] == id.as_str()
                            && card["updated_ms"]
                                .as_u64()
                                .is_some_and(|stamp| stamp >= self.news_requested_at)
                    })
            })
            .collect();
        self.next_news_batch()
    }
    pub fn begin_news(&mut self, sources: Vec<String>) -> Vec<String> {
        self.news_requested_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.news_interrupted.clear();
        self.pending = sources.into();
        self.next_news_batch()
    }
}

impl Section {
    pub fn select_page(&mut self, state: &PageState) {
        if matches!(self, Self::Settings(Settings::Page)) {
            *self = Self::Settings(state.last_settings.unwrap_or(Settings::Page));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resume_only_fetches_interrupted_visible_sources() {
        let mut page = PageState::default();
        page.catalog = ["a", "b", "c", "d", "e"]
            .into_iter()
            .map(|id| serde_json::json!({"id":id}))
            .collect();
        let batch = page.begin_news(page.visible_sources());
        assert_eq!(batch, ["a", "b", "c", "d"]);
        page.pause_news();
        page.cards
            .push(serde_json::json!({"id":"a","updated_ms":page.news_requested_at}));
        page.news_preferences = serde_json::json!({"hidden":["b"]});
        assert_eq!(page.resume_news(), ["c", "d", "e"]);
    }
    #[test]
    fn switching_pages_preserves_global_settings_category() {
        let state = PageState::default();
        for global in [Settings::Display, Settings::System, Settings::About] {
            let mut section = Section::Settings(global);
            section.select_page(&state);
            assert_eq!(section, Section::Settings(global));
        }
        let mut section = Section::Settings(Settings::Page);
        section.select_page(&state);
        assert_eq!(section, Section::Settings(Settings::Page));
    }
}
