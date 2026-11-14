//! Host-independent feature package contract. Service data never contains UI objects.
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

pub const API_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Feature,
    Runtime,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub service_api: u32,
    pub target: String,
    pub entrypoint: String,
    #[serde(default)]
    pub kind: Kind,
    pub page: Option<Page>,
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "template", rename_all = "snake_case", deny_unknown_fields)]
pub enum Page {
    NewsGrid {
        catalog_method: String,
        fetch_method: String,
    },
    MediaLibrary {
        search_method: String,
        favorites_method: String,
    },
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && id.as_bytes()[0].is_ascii_lowercase()
}

pub fn package_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':'])
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

impl Manifest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != API_VERSION || self.service_api != API_VERSION {
            return Err("unsupported_protocol");
        }
        if !valid_id(&self.id) || self.name.trim().is_empty() {
            return Err("invalid_identity");
        }
        if self.target != "windows-x86_64" {
            return Err("unsupported_platform");
        }
        if !package_path(&self.entrypoint) {
            return Err("invalid_entrypoint");
        }
        if !version(&self.version) {
            return Err("invalid_version");
        }
        if (self.kind == Kind::Feature) != self.page.is_some()
            || (self.kind == Kind::Runtime && self.runtime.is_some())
        {
            return Err("invalid_package_kind");
        }
        let mut ids = std::collections::BTreeSet::new();
        for dependency in &self.dependencies {
            if !valid_id(&dependency.id)
                || dependency.id == self.id
                || !version(&dependency.version)
                || !ids.insert(&dependency.id)
            {
                return Err("invalid_dependency");
            }
        }
        if let Some(runtime) = &self.runtime
            && !self
                .dependencies
                .iter()
                .any(|dependency| &dependency.id == runtime)
        {
            return Err("undeclared_runtime");
        }
        Ok(())
    }
}

pub fn version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0'))
                && part.parse::<u32>().is_ok()
        })
}

/// A visible activation owns its replies; old A -> B -> A results are discarded.
#[derive(Default, Debug)]
pub struct Activation {
    generation: u64,
    current: Option<String>,
}

impl Activation {
    pub fn select(&mut self, id: Option<&str>) -> u64 {
        if self.current.as_deref() != id {
            self.generation += 1;
            self.current = id.map(str::to_owned);
        }
        self.generation
    }

    pub fn accepts(&self, id: &str, generation: u64) -> bool {
        self.current.as_deref() == Some(id) && self.generation == generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        serde_json::from_str(r#"{"schema":1,"id":"news","name":"新闻","version":"0.1.0","service_api":1,"target":"windows-x86_64","entrypoint":"bin/news.exe","page":{"template":"news_grid","catalog_method":"sources","fetch_method":"fetch"}}"#).unwrap()
    }

    #[test]
    fn manifest_checks_protocol_platform_and_dependency_identity() {
        let mut m = manifest();
        assert_eq!(m.validate(), Ok(()));
        m.service_api = 2;
        assert_eq!(m.validate(), Err("unsupported_protocol"));
        m.service_api = 1;
        m.dependencies.push(Dependency {
            id: "news".into(),
            version: "0.1.0".into(),
        });
        assert_eq!(m.validate(), Err("invalid_dependency"));
    }

    #[test]
    fn paths_and_ids_cannot_escape_a_package() {
        for path in [
            "../secret",
            "/outside",
            "C:/outside",
            "bin\\..\\secret",
            "bin/file:stream",
            "",
        ] {
            assert!(!package_path(path), "{path}");
        }
        for id in ["../news", "News", "-news", "", "news/child"] {
            assert!(!valid_id(id));
        }
        assert!(package_path("bin/news.exe"));
    }

    #[test]
    fn versions_are_exact_and_canonical() {
        for value in ["1.0.0", "0.1.12"] {
            assert!(version(value));
        }
        for value in ["1", "01.0.0", "1.0.0-beta", "1..0", "1.0.4294967296"] {
            assert!(!version(value));
        }
    }

    #[test]
    fn interpreter_must_be_a_declared_runtime_dependency() {
        let mut m = manifest();
        m.runtime = Some("python".into());
        assert_eq!(m.validate(), Err("undeclared_runtime"));
        m.dependencies.push(Dependency {
            id: "python".into(),
            version: "3.13.0".into(),
        });
        assert_eq!(m.validate(), Ok(()));
        m.kind = Kind::Runtime;
        assert_eq!(m.validate(), Err("invalid_package_kind"));
        m.page = None;
        m.runtime = None;
        assert_eq!(m.validate(), Ok(()));
    }

    #[test]
    fn reactivation_does_not_accept_old_results() {
        let mut activation = Activation::default();
        let first = activation.select(Some("news"));
        assert_eq!(activation.select(Some("news")), first);
        activation.select(Some("hongguo"));
        let third = activation.select(Some("news"));
        assert!(!activation.accepts("news", first));
        assert!(activation.accepts("news", third));
        activation.select(None);
        assert!(!activation.accepts("news", third));
    }
}
