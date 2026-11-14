use super::*;
use lazybuddy_app_updater::Prepared;

pub(super) struct State {
    pub cache: PathBuf,
    pub prepared: Option<Prepared>,
    pub checking: bool,
    pub applying: bool,
    pub notice: String,
}
impl State {
    pub fn new(cache: PathBuf) -> Self {
        let failed = std::fs::read(cache.join("result.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|result| result["ok"] == false);
        Self {
            cache,
            prepared: None,
            checking: false,
            applying: false,
            notice: if failed {
                "上次更新未完成，可重新检查。".into()
            } else {
                String::new()
            },
        }
    }
    pub fn begin(&mut self) -> bool {
        if self.checking || self.applying {
            return false;
        }
        self.checking = true;
        self.notice = "正在检查并准备应用更新…".into();
        true
    }
    pub fn complete(&mut self, result: Result<Option<Prepared>, String>) {
        self.checking = false;
        match result {
            Ok(Some(prepared)) => {
                self.notice = format!("应用更新 {} 已就绪，点击重启更新。", prepared.version);
                self.prepared = Some(prepared);
            }
            Ok(None) => {
                self.notice = "应用已是最新构建。".into();
                self.prepared = None;
            }
            Err(error) => self.notice = error,
        }
    }
    pub fn can_apply(&self) -> bool {
        self.prepared.is_some() && !self.checking && !self.applying
    }
}
impl App {
    pub(super) fn check_updates(&mut self) -> Task<Message> {
        if self.quitting || !self.updater.begin() {
            return Task::none();
        }
        let cache = self.updater.cache.clone();
        Task::perform(
            background(move || {
                let executable =
                    std::env::current_exe().map_err(|_| "无法确认应用位置。".to_owned())?;
                lazybuddy_app_updater::prepare(executable.parent().unwrap(), &cache)
                    .map_err(|error| format!("应用更新未完成：{error}"))
            }),
            Message::UpdateChecked,
        )
    }
    pub(super) fn apply_update(&mut self) -> Task<Message> {
        if !self.updater.can_apply() || self.quitting {
            return Task::none();
        }
        let prepared = self.updater.prepared.clone().unwrap();
        self.updater.applying = true;
        self.updater.notice = "正在准备重启更新…".into();
        Task::perform(
            background(move || {
                let executable =
                    std::env::current_exe().map_err(|_| "无法确认应用位置。".to_owned())?;
                lazybuddy_app_updater::start_helper(&prepared, executable.parent().unwrap())
                    .map_err(|error| format!("无法准备更新，应用仍在运行：{error}"))
            }),
            Message::UpdateStarted,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> State {
        State {
            cache: PathBuf::new(),
            prepared: None,
            checking: false,
            applying: false,
            notice: String::new(),
        }
    }
    fn prepared() -> Prepared {
        Prepared {
            directory: "fixture".into(),
            version: "2.0.0".into(),
            build: "a".repeat(64),
        }
    }
    #[test]
    fn checks_are_single_flight_and_failed_recheck_keeps_ready_update() {
        let mut state = state();
        assert!(state.begin());
        assert!(!state.begin());
        state.complete(Ok(Some(prepared())));
        assert!(state.can_apply());
        assert!(state.begin());
        assert!(!state.can_apply());
        state.complete(Err("offline".into()));
        assert!(state.can_apply());
        assert_eq!(state.prepared.unwrap().build, "a".repeat(64));
    }
    #[test]
    fn current_result_clears_stale_offer_and_applying_blocks_checks() {
        let mut state = state();
        state.complete(Ok(Some(prepared())));
        state.complete(Ok(None));
        assert!(!state.can_apply());
        state.complete(Ok(Some(prepared())));
        state.applying = true;
        assert!(!state.can_apply());
        assert!(!state.begin());
    }
}
