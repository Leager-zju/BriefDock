//! Versioned package storage with one atomic activation registry.
pub mod package;
mod registry;
use lazybuddy_module_contract::{Kind, Manifest, valid_id};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
pub struct Manager {
    root: PathBuf,
}
pub struct Launch {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
}
pub(crate) struct Staged {
    pub path: PathBuf,
    pub manifest: Manifest,
}

impl Manager {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn lock(&self) -> io::Result<fs::File> {
        fs::create_dir_all(&self.root)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join(".lock"))?;
        file.try_lock()
            .map_err(|_| invalid("another module operation is running"))?;
        let registry = registry::read(&self.root)?;
        // Old running executables may defer deletion; activation remains committed.
        let _ = registry::prune(&self.root, &registry);
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if [".install-", ".unpack-"]
                .iter()
                .any(|prefix| name.strip_prefix(prefix).is_some_and(valid_id))
            {
                registry::plain_directory(&entry.path())?;
                fs::remove_dir_all(entry.path())?;
            } else if name
                .strip_prefix(".download-")
                .and_then(|name| name.strip_suffix(".zip"))
                .is_some_and(valid_id)
                && entry.file_type()?.is_file()
            {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(file)
    }
    fn directory(&self, id: &str, version: &str) -> PathBuf {
        self.root.join("packages").join(id).join(version)
    }
    fn manifests(&self, registry: &registry::Registry) -> io::Result<BTreeMap<String, Manifest>> {
        registry
            .iter()
            .map(|(id, version)| {
                let manifest = read_manifest(&self.directory(id, version))?;
                if manifest.id != *id || manifest.version != *version {
                    return Err(invalid("installed package identity mismatch"));
                }
                Ok((id.clone(), manifest))
            })
            .collect()
    }
    pub fn manifest(&self, id: &str) -> io::Result<Manifest> {
        if !valid_id(id) {
            return Err(invalid("invalid module ID"));
        }
        let registry = registry::read(&self.root)?;
        let version = registry
            .get(id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "module not installed"))?;
        let manifest = read_manifest(&self.directory(id, version))?;
        if manifest.id != id || manifest.version != *version {
            return Err(invalid("installed package identity mismatch"));
        }
        Ok(manifest)
    }
    pub fn list(&self) -> io::Result<Vec<Manifest>> {
        if !self.root.exists() {
            return Ok(vec![]);
        }
        let _lock = self.lock()?;
        Ok(self
            .manifests(&registry::read(&self.root)?)?
            .into_values()
            .collect())
    }
    pub(crate) fn activate_many(&self, staged: &[Staged]) -> io::Result<Vec<Manifest>> {
        let mut registry = registry::read(&self.root)?;
        let mut manifests = self.manifests(&registry)?;
        for package in staged {
            if !package.path.join(&package.manifest.entrypoint).is_file() {
                return Err(invalid("service entrypoint missing"));
            }
            if registry.get(&package.manifest.id) == Some(&package.manifest.version) {
                return Err(invalid("module version already installed"));
            }
            manifests.insert(package.manifest.id.clone(), package.manifest.clone());
            registry.insert(
                package.manifest.id.clone(),
                package.manifest.version.clone(),
            );
        }
        for manifest in manifests.values() {
            for dependency in &manifest.dependencies {
                if !manifests
                    .get(&dependency.id)
                    .is_some_and(|provider| provider.version == dependency.version)
                {
                    return Err(invalid("dependency version mismatch"));
                }
            }
            if manifest
                .runtime
                .as_ref()
                .is_some_and(|runtime| manifests[runtime].kind != Kind::Runtime)
            {
                return Err(invalid("runtime provider required"));
            }
        }
        let mut remaining: std::collections::BTreeSet<_> = manifests.keys().cloned().collect();
        while !remaining.is_empty() {
            let ready: Vec<_> = remaining
                .iter()
                .filter(|id| {
                    manifests[*id]
                        .dependencies
                        .iter()
                        .all(|dependency| !remaining.contains(&dependency.id))
                })
                .cloned()
                .collect();
            if ready.is_empty() {
                return Err(invalid("cyclic installed dependency"));
            }
            for id in ready {
                remaining.remove(&id);
            }
        }
        for package in staged {
            let destination = self.directory(&package.manifest.id, &package.manifest.version);
            fs::create_dir_all(destination.parent().expect("package ID directory"))?;
            fs::rename(&package.path, destination)?;
        }
        registry::write(&self.root, &registry)?;
        let _ = registry::prune(&self.root, &registry);
        Ok(staged
            .iter()
            .map(|package| package.manifest.clone())
            .collect())
    }
    pub fn install_local(&self, source: &Path) -> io::Result<Manifest> {
        let _lock = self.lock()?;
        if fs::canonicalize(&self.root)?.starts_with(fs::canonicalize(source)?) {
            return Err(invalid("package source contains installation directory"));
        }
        let manifest = read_manifest(source)?;
        let stage = self.root.join(format!(".install-{}", manifest.id));
        fs::create_dir(&stage)?;
        let result = (|| {
            copy_tree(source, &stage, &mut (2 * 1024 * 1024 * 1024u64))?;
            let copied = read_manifest(&stage)?;
            if copied.id != manifest.id || copied.version != manifest.version {
                return Err(invalid("package changed during copy"));
            }
            self.activate_many(&[Staged {
                path: stage.clone(),
                manifest: copied,
            }])?
            .pop()
            .ok_or_else(|| invalid("missing activation"))
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(stage);
        }
        result
    }
    pub fn uninstall(&self, id: &str) -> io::Result<()> {
        let _lock = self.lock()?;
        self.manifest(id)?;
        let mut registry = registry::read(&self.root)?;
        let mut manifests = self.manifests(&registry)?;
        if manifests.values().any(|module| {
            module
                .dependencies
                .iter()
                .any(|dependency| dependency.id == id)
        }) {
            return Err(invalid("module still required by another installed module"));
        }
        registry.remove(id);
        manifests.remove(id);
        loop {
            let orphan = manifests
                .values()
                .find(|module| {
                    module.kind == Kind::Runtime
                        && !manifests.values().any(|other| {
                            other
                                .dependencies
                                .iter()
                                .any(|dependency| dependency.id == module.id)
                        })
                })
                .map(|module| module.id.clone());
            let Some(orphan) = orphan else { break };
            registry.remove(&orphan);
            manifests.remove(&orphan);
        }
        registry::write(&self.root, &registry)?;
        registry::prune(&self.root, &registry).map_err(|error| {
            io::Error::new(
                error.kind(),
                "module detached; old payload cleanup is pending",
            )
        })
    }
    pub fn entrypoint(&self, id: &str) -> io::Result<PathBuf> {
        let manifest = self.manifest(id)?;
        Ok(self
            .directory(id, &manifest.version)
            .join(manifest.entrypoint))
    }
    pub fn launch(&self, id: &str) -> io::Result<Launch> {
        let manifest = self.manifest(id)?;
        let own = self
            .directory(id, &manifest.version)
            .join(&manifest.entrypoint);
        let mut environment = vec![(
            "LAZYBUDDY_MODULES_DIR".into(),
            self.root
                .to_str()
                .ok_or_else(|| invalid("module directory is not UTF-8"))?
                .to_owned(),
        )];
        for dependency in &manifest.dependencies {
            let provider = self.manifest(&dependency.id)?;
            let directory = self.directory(&provider.id, &provider.version);
            environment.push((
                format!(
                    "LAZYBUDDY_DEPENDENCY_{}_DIR",
                    dependency.id.replace('-', "_").to_ascii_uppercase()
                ),
                directory
                    .to_str()
                    .ok_or_else(|| invalid("dependency directory is not UTF-8"))?
                    .to_owned(),
            ));
        }
        if let Some(runtime) = &manifest.runtime {
            let provider = self.manifest(runtime)?;
            if provider.kind != Kind::Runtime {
                return Err(invalid("runtime provider required"));
            }
            let mut arguments = provider.arguments;
            arguments.push(
                own.to_str()
                    .ok_or_else(|| invalid("entrypoint is not UTF-8"))?
                    .to_owned(),
            );
            arguments.extend(manifest.arguments);
            Ok(Launch {
                executable: self
                    .directory(runtime, &provider.version)
                    .join(provider.entrypoint),
                arguments,
                environment,
            })
        } else {
            Ok(Launch {
                executable: own,
                arguments: manifest.arguments,
                environment,
            })
        }
    }
}

fn read_manifest(directory: &Path) -> io::Result<Manifest> {
    if fs::symlink_metadata(directory)?.file_type().is_symlink() {
        return Err(invalid("linked package directory"));
    }
    let path = directory.join("module.json");
    if fs::symlink_metadata(&path)?.file_type().is_symlink() || fs::metadata(&path)?.len() > 65536 {
        return Err(invalid("invalid manifest file"));
    }
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(path)?).map_err(|_| invalid("invalid manifest JSON"))?;
    manifest.validate().map_err(invalid)?;
    Ok(manifest)
}

fn copy_tree(source: &Path, destination: &Path, remaining: &mut u64) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid("linked package path"));
        }
    }
    if metadata.file_type().is_symlink() {
        return Err(invalid("linked package path"));
    }
    if !metadata.is_dir() {
        return Err(invalid("expected package directory"));
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(invalid("linked package path"));
            }
        }
        let target = destination.join(entry.file_name());
        if metadata.file_type().is_symlink() {
            return Err(invalid("linked package path"));
        }
        if metadata.is_dir() {
            fs::create_dir(&target)?;
            copy_tree(&entry.path(), &target, remaining)?;
        } else if metadata.is_file() {
            *remaining = remaining
                .checked_sub(metadata.len())
                .ok_or_else(|| invalid("package exceeds size limit"))?;
            fs::copy(entry.path(), target)?;
        } else {
            return Err(invalid("unsupported package entry"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_and_shared_runtime_activate_as_one_transaction() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("qa-module-transaction-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let manager = Manager::new(root.join("modules"));
        let stage = |id: &str, version: &str, runtime: Option<&str>| {
            let path = root.join(format!("{id}-{version}"));
            fs::create_dir_all(path.join("bin")).unwrap();
            let mut manifest: Manifest =
                serde_json::from_str(include_str!("../../../contracts/fixtures/feature.json"))
                    .unwrap();
            manifest.id = id.into();
            manifest.version = version.into();
            if id == "python" {
                manifest.kind = Kind::Runtime;
                manifest.page = None;
                manifest.arguments = vec!["-I".into()];
            }
            if let Some(version) = runtime {
                manifest
                    .dependencies
                    .push(lazybuddy_module_contract::Dependency {
                        id: "python".into(),
                        version: version.into(),
                    });
                manifest.runtime = Some("python".into());
            }
            fs::write(
                path.join("module.json"),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
            fs::write(path.join(&manifest.entrypoint), b"fixture; never executed").unwrap();
            Staged { path, manifest }
        };
        let first = vec![
            stage("python", "1.0.0", None),
            stage("feature", "1.0.0", Some("1.0.0")),
        ];
        {
            let _lock = manager.lock().unwrap();
            manager.activate_many(&first).unwrap();
        }
        let launch = manager.launch("feature").unwrap();
        assert_eq!(launch.arguments[0], "-I");
        assert!(launch.arguments[1].contains("feature"));
        let second = vec![
            stage("python", "2.0.0", None),
            stage("feature", "2.0.0", Some("2.0.0")),
        ];
        {
            let _lock = manager.lock().unwrap();
            assert!(manager.activate_many(&second[..1]).is_err());
        }
        assert_eq!(manager.manifest("python").unwrap().version, "1.0.0");
        fs::create_dir(manager.root.join("registry.tmp")).unwrap();
        {
            let _lock = manager.lock().unwrap();
            assert!(manager.activate_many(&second).is_err());
        }
        assert_eq!(manager.manifest("feature").unwrap().version, "1.0.0");
        assert_eq!(manager.manifest("python").unwrap().version, "1.0.0");
        fs::remove_dir(manager.root.join("registry.tmp")).unwrap();
        manager.list().unwrap();
        let second = vec![
            stage("python", "2.0.0", None),
            stage("feature", "2.0.0", Some("2.0.0")),
        ];
        {
            let _lock = manager.lock().unwrap();
            manager.activate_many(&second).unwrap();
        }
        assert_eq!(manager.manifest("feature").unwrap().version, "2.0.0");
        assert_eq!(manager.manifest("python").unwrap().version, "2.0.0");
        manager.uninstall("feature").unwrap();
        assert!(manager.list().unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_install_is_atomic_and_dependencies_prevent_removal() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!(
                "lazybuddy-manager-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let source = root.join("source");
        fs::create_dir_all(source.join("bin")).unwrap();
        let mut manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../contracts/fixtures/feature.json")).unwrap();
        fs::write(
            source.join("module.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let manager = Manager::new(root.join("installed"));
        assert!(manager.install_local(&source).is_err());
        assert!(manager.list().unwrap().is_empty());
        assert!(!manager.root.join(".install-news").exists());
        fs::write(source.join("bin/service.exe"), b"fixture, not executable").unwrap();
        manager.install_local(&source).unwrap();
        assert!(manager.install_local(&source).is_err());
        manifest["id"] = "dependent".into();
        manifest["dependencies"] = serde_json::json!([{"id":"sample","version":"0.1.0"}]);
        fs::write(
            source.join("module.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        manager.install_local(&source).unwrap();
        assert!(manager.uninstall("sample").is_err());
        manager.uninstall("dependent").unwrap();
        manifest["id"] = "sample".into();
        manifest["version"] = "0.2.0".into();
        manifest["dependencies"] = serde_json::json!([]);
        fs::write(
            source.join("module.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let held = manager.lock().unwrap();
        assert!(manager.install_local(&source).is_err());
        drop(held);
        // Fail publication after the new payload is staged; the old registry stays active.
        fs::create_dir(manager.root.join("registry.tmp")).unwrap();
        assert!(manager.install_local(&source).is_err());
        assert_eq!(manager.manifest("sample").unwrap().version, "0.1.0");
        fs::remove_dir(manager.root.join("registry.tmp")).unwrap();
        manager.list().unwrap();
        assert!(!manager.directory("sample", "0.2.0").exists());
        manager.install_local(&source).unwrap();
        assert_eq!(manager.manifest("sample").unwrap().version, "0.2.0");
        manager.uninstall("sample").unwrap();
        assert!(manager.list().unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
