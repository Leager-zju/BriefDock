use super::*;
use lazybuddy_module_manager::package::{Source, download_catalog};

impl App {
    pub(super) fn load_catalog(&mut self) -> Task<Message> {
        self.manager_busy = true;
        Task::perform(
            background(|| {
                let path =
                    std::env::current_exe().map_err(|_| "功能目录暂时无法访问。".to_owned())?;
                let source = Source::load(&path.with_file_name("module-catalog.json"))
                    .map_err(|_| "功能目录暂时无法访问。".to_owned())?;
                download_catalog(&source.url, &source.public_key)
                    .map_err(|_| "功能目录读取失败，请稍后重试。".into())
            }),
            Message::CatalogLoaded,
        )
    }

    pub(super) fn install_module(&mut self, id: String) -> Task<Message> {
        let Some(catalog) = self.catalog.clone() else {
            return self.load_catalog();
        };
        self.manager_busy = true;
        self.error = None;
        let manager = Manager::new(self.manager.root().to_owned());
        let closing = self.closing_sessions.clone();
        Task::perform(
            async move {
                for completion in closing {
                    completion.await;
                }
                background(move || {
                    manager
                        .install_from_catalog(&catalog, &id)
                        .map(|_| ())
                        .map_err(|error| format!("安装未完成：{error}。已安装功能保留，请重试。"))
                })
                .await
            },
            Message::Removed,
        )
    }
}
