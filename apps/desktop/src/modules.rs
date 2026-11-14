use super::*;
use lazybuddy_module_manager::package::{Source, download_catalog};

pub(super) struct FeatureEntry<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub installed: bool,
    pub update: bool,
}

pub(super) fn feature_entries<'a>(
    installed: &'a [Manifest],
    catalog: Option<&'a lazybuddy_module_manager::package::Catalog>,
) -> Vec<FeatureEntry<'a>> {
    let mut entries = BTreeMap::new();
    for module in installed
        .iter()
        .filter(|module| module.kind == Kind::Feature)
    {
        entries.insert(
            module.id.as_str(),
            FeatureEntry {
                id: &module.id,
                name: &module.name,
                version: &module.version,
                installed: true,
                update: false,
            },
        );
    }
    for package in catalog
        .into_iter()
        .flat_map(|catalog| &catalog.packages)
        .filter(|package| package.kind == Kind::Feature)
    {
        let name = if package.name.is_empty() {
            &package.id
        } else {
            &package.name
        };
        if let Some(entry) = entries.get_mut(package.id.as_str()) {
            // Manifest/catalog versions are already validated canonical u32 triples.
            entry.update = package
                .version
                .split('.')
                .map(|part| part.parse::<u32>().unwrap())
                .cmp(
                    entry
                        .version
                        .split('.')
                        .map(|part| part.parse::<u32>().unwrap()),
                )
                .is_gt();
        } else {
            entries.insert(
                package.id.as_str(),
                FeatureEntry {
                    id: &package.id,
                    name,
                    version: &package.version,
                    installed: false,
                    update: false,
                },
            );
        }
    }
    entries.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feature_list_merges_installed_and_available_without_duplicates_or_downgrades() {
        let manifest = |id, version| {
            let mut value: Value =
                serde_json::from_str(include_str!("../../../contracts/fixtures/feature.json"))
                    .unwrap();
            value["id"] = json!(id);
            value["name"] = json!(id);
            value["version"] = json!(version);
            serde_json::from_value::<Manifest>(value).unwrap()
        };
        let installed = vec![
            manifest("alpha", "0.9.0"),
            manifest("beta", "10.0.0"),
            manifest("offline", "1.0.0"),
        ];
        let catalog = serde_json::from_value(json!({"schema":1,"packages":[
            {"id":"alpha","version":"0.10.0","target":"windows-x86_64","url":"https://example.invalid/a.zip","sha256":"0".repeat(64),"bytes":1},
            {"id":"beta","version":"2.0.0","target":"windows-x86_64","url":"https://example.invalid/b.zip","sha256":"0".repeat(64),"bytes":1},
            {"id":"new","version":"1.0.0","target":"windows-x86_64","url":"https://example.invalid/n.zip","sha256":"0".repeat(64),"bytes":1},
            {"id":"runtime","kind":"runtime","version":"1.0.0","target":"windows-x86_64","url":"https://example.invalid/r.zip","sha256":"0".repeat(64),"bytes":1}
        ]})).unwrap();
        let entries = feature_entries(&installed, Some(&catalog));
        assert_eq!(
            entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            ["alpha", "beta", "new", "offline"]
        );
        assert!(entries[0].installed && entries[0].update);
        assert_eq!(entries[0].version, "0.9.0");
        assert!(entries[1].installed && !entries[1].update);
        assert!(!entries[2].installed);
        assert!(entries[3].installed && !entries[3].update);
        assert_eq!(feature_entries(&installed, None).len(), 3);
    }
}

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
