//! One atomic file selects all installed versions, including shared dependencies.
use crate::invalid;
use lazybuddy_module_contract::{valid_id, version};
use std::{collections::BTreeMap, fs, io, path::Path};

pub type Registry = BTreeMap<String, String>;

pub fn read(root: &Path) -> io::Result<Registry> {
    let path = root.join("registry.json");
    match fs::metadata(&path) {
        Ok(metadata) if metadata.len() <= 65536 => {}
        Ok(_) => return Err(invalid("module registry too large")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Registry::new()),
        Err(error) => return Err(error),
    }
    let registry: Registry =
        serde_json::from_slice(&fs::read(path)?).map_err(|_| invalid("invalid module registry"))?;
    if registry
        .iter()
        .any(|(id, value)| !valid_id(id) || !version(value))
    {
        return Err(invalid("invalid installed version"));
    }
    Ok(registry)
}

pub fn write(root: &Path, registry: &Registry) -> io::Result<()> {
    let temporary = root.join("registry.tmp");
    let mut file = fs::File::create(&temporary)?;
    serde_json::to_writer(&mut file, registry)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, root.join("registry.json"))
}

pub fn prune(root: &Path, registry: &Registry) -> io::Result<()> {
    let store = root.join("packages");
    if !store.exists() {
        return Ok(());
    }
    plain_directory(&store)?;
    for package in fs::read_dir(store)? {
        let package = package?;
        let id = package.file_name();
        let Some(id) = id.to_str().filter(|id| valid_id(id)) else {
            return Err(invalid("unexpected package directory"));
        };
        plain_directory(&package.path())?;
        if !package.file_type()?.is_dir() {
            return Err(invalid("invalid package directory"));
        }
        for entry in fs::read_dir(package.path())? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(value) = name.to_str().filter(|value| version(value)) else {
                return Err(invalid("unexpected package version"));
            };
            if registry.get(id).is_some_and(|current| current == value) {
                continue;
            }
            if !entry.file_type()?.is_dir() {
                return Err(invalid("invalid package version directory"));
            }
            plain_directory(&entry.path())?;
            fs::remove_dir_all(entry.path())?;
        }
        if fs::read_dir(package.path())?.next().is_none() {
            fs::remove_dir(package.path())?;
        }
    }
    Ok(())
}

pub(crate) fn plain_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid("linked package directory"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid("reparse package directory"));
        }
    }
    Ok(())
}
