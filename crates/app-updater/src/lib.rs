//! Signed portable-host updates. Feature packages and user data are never replaced.
#[cfg(test)]
mod tests;
mod windows;
use lazybuddy_module_contract::{package_path, version};
use lazybuddy_module_manager::package::{Source, download_archive, download_signed_payload};
use ring::signature::{ED25519, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const SOURCE: &str = "app-update-source.json";
const INFO: &str = "build-info.json";
const REQUIRED: [&str; 5] = [
    "lazybuddy.exe",
    "lazybuddy-cli.exe",
    "lazybuddy-updater.exe",
    SOURCE,
    "module-catalog.json",
];
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub target: String,
    pub version: String,
    pub build: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
    pub host_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildInfo {
    pub schema: u32,
    pub version: String,
    pub build: String,
    pub files: BTreeMap<String, String>,
}
#[derive(Clone, Debug)]
pub struct Prepared {
    pub directory: PathBuf,
    pub version: String,
    pub build: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    install: PathBuf,
    parent_pid: u32,
}

fn hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn file_digest(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> io::Result<T> {
    if fs::metadata(path)?.len() > 1024 * 1024 {
        return Err(invalid("update metadata too large"));
    }
    serde_json::from_slice(&fs::read(path)?).map_err(|_| invalid("invalid update metadata"))
}
pub fn verify_manifest(payload: &[u8], signature: &[u8], key: &[u8]) -> io::Result<Manifest> {
    if payload.len() > 1024 * 1024 {
        return Err(invalid("update manifest too large"));
    }
    UnparsedPublicKey::new(&ED25519, key)
        .verify(payload, signature)
        .map_err(|_| invalid("invalid update signature"))?;
    let manifest: Manifest =
        serde_json::from_slice(payload).map_err(|_| invalid("invalid update manifest"))?;
    if manifest.schema != 1
        || manifest.target != "windows-x86_64"
        || !version(&manifest.version)
        || !hex(&manifest.build)
        || !hex(&manifest.sha256)
        || !hex(&manifest.host_sha256)
        || !manifest.url.starts_with("https://")
        || manifest.bytes == 0
        || manifest.bytes > 256 * 1024 * 1024
    {
        return Err(invalid("unsupported update manifest"));
    }
    Ok(manifest)
}
fn owned(name: &str) -> bool {
    package_path(name)
        && (REQUIRED.contains(&name)
            || matches!(name, "README.txt" | "LICENSE" | INFO)
            || name.starts_with("licenses/"))
}
fn plain(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid("linked update path"));
        }
    }
    if metadata.file_type().is_symlink() {
        return Err(invalid("linked update path"));
    }
    Ok(())
}
fn destination(root: &Path, name: &str) -> io::Result<PathBuf> {
    if !owned(name) {
        return Err(invalid("unexpected application file"));
    }
    plain(root)?;
    let mut path = root.to_owned();
    for part in Path::new(name).components() {
        path.push(part);
        match plain(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(path)
}
pub fn verify_stage(stage: &Path, manifest: &Manifest) -> io::Result<BuildInfo> {
    let info: BuildInfo = read_json(&stage.join(INFO))?;
    if info.schema != 1
        || info.version != manifest.version
        || info.build != manifest.build
        || info.files.len() > 10000
        || digest(&serde_json::to_vec(&info.files).map_err(io::Error::other)?) != info.build
        || info.files.get("lazybuddy.exe") != Some(&manifest.host_sha256)
        || REQUIRED.iter().any(|name| !info.files.contains_key(*name))
    {
        return Err(invalid("application build mismatch"));
    }
    for (name, expected) in &info.files {
        if !hex(expected) || file_digest(&destination(stage, name)?)? != *expected {
            return Err(invalid("application file digest mismatch"));
        }
    }
    Ok(info)
}
fn current(install: &Path, manifest: &Manifest) -> bool {
    read_json::<BuildInfo>(&install.join(INFO)).is_ok_and(|info| info.build == manifest.build)
        && file_digest(&install.join("lazybuddy.exe"))
            .is_ok_and(|hash| hash == manifest.host_sha256)
}
fn unpack(archive: &Path, stage: &Path, manifest: &Manifest) -> io::Result<()> {
    if fs::metadata(archive)?.len() != manifest.bytes || file_digest(archive)? != manifest.sha256 {
        return Err(invalid("application archive digest mismatch"));
    }
    fs::create_dir(stage)?;
    let mut archive = zip::ZipArchive::new(fs::File::open(archive)?)
        .map_err(|_| invalid("invalid application ZIP"))?;
    if archive.len() > 10000 {
        return Err(invalid("too many application files"));
    }
    let mut budget = 512 * 1024 * 1024u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| invalid("invalid application ZIP entry"))?;
        let name = entry
            .name()
            .strip_prefix("LazyBuddy/")
            .ok_or_else(|| invalid("unexpected application ZIP root"))?;
        if !owned(name)
            || entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(invalid("unsafe application ZIP path"));
        }
        budget = budget
            .checked_sub(entry.size())
            .ok_or_else(|| invalid("application ZIP too large"))?;
        let path = destination(stage, name)?;
        fs::create_dir_all(path.parent().unwrap())?;
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        let expected = entry.size();
        if io::copy(&mut entry.take(expected + 1), &mut output)? != expected {
            return Err(invalid("incomplete application ZIP entry"));
        }
    }
    verify_stage(stage, manifest)?;
    Ok(())
}
fn clear_directory(path: &Path) -> io::Result<()> {
    plain(path)?;
    // This function is used only on updater-owned cache directories, never installs.
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        plain(&entry.path())?;
        if entry.file_type()?.is_dir() {
            clear_directory(&entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    fs::remove_dir(path)
}
fn prune(cache: &Path, keep: Option<&str>) {
    if let Ok(entries) = fs::read_dir(cache) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if hex(&name) && Some(name.as_ref()) != keep {
                let lock = entry.path().join("apply.lock");
                if lock.exists() && windows::lock(&lock).is_err() {
                    continue;
                }
                let _ = clear_directory(&entry.path());
            }
        }
    }
}
pub fn prepare(install: &Path, cache: &Path) -> io::Result<Option<Prepared>> {
    let source = Source::load(&install.join(SOURCE))?;
    let (payload, signature) = download_signed_payload(&source.url, &source.public_key)?;
    let manifest = verify_manifest(&payload, &signature, &source.public_key)?;
    if current(install, &manifest) {
        prune(cache, None);
        return Ok(None);
    }
    fs::create_dir_all(cache)?;
    plain(cache)?;
    let directory = cache.join(&manifest.build);
    let stage = directory.join("stage");
    if verify_stage(&stage, &manifest).is_err() {
        if directory.exists() {
            clear_directory(&directory)?;
        }
        fs::create_dir(&directory)?;
        let archive = directory.join("package.zip");
        let result = (|| {
            download_archive(&manifest.url, &archive, manifest.bytes)?;
            unpack(&archive, &stage, &manifest)
        })();
        if let Err(error) = result {
            let _ = clear_directory(&directory);
            return Err(error);
        }
    }
    fs::write(directory.join("manifest.json"), payload)?;
    fs::write(directory.join("manifest.json.sig"), signature)?;
    prune(cache, Some(&manifest.build));
    Ok(Some(Prepared {
        directory,
        version: manifest.version,
        build: manifest.build,
    }))
}
pub fn start_helper(prepared: &Prepared, install: &Path) -> io::Result<()> {
    let directory = &prepared.directory;
    plain(directory)?;
    let source = Source::load(&install.join(SOURCE))?;
    let manifest = verify_manifest(
        &fs::read(directory.join("manifest.json"))?,
        &fs::read(directory.join("manifest.json.sig"))?,
        &source.public_key,
    )?;
    verify_stage(&directory.join("stage"), &manifest)?;
    let ready = directory.join("ready");
    if ready.exists() {
        fs::remove_file(&ready)?;
    }
    fs::write(
        directory.join("job.json"),
        serde_json::to_vec(&Job {
            install: install.canonicalize()?,
            parent_pid: std::process::id(),
        })
        .map_err(io::Error::other)?,
    )?;
    fs::copy(
        directory.join("stage/lazybuddy-updater.exe"),
        directory.join("apply.exe"),
    )?;
    let mut child = windows::command(&directory.join("apply.exe"))
        .arg("--apply")
        .arg(directory.join("job.json"))
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if ready.is_file() {
            return Ok(());
        }
        if child.try_wait()?.is_some() {
            return Err(invalid("update helper could not prepare restart"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(invalid("update helper startup timed out"))
}

struct Transaction {
    install: PathBuf,
    backup: PathBuf,
    changed: Vec<(String, bool)>,
}
impl Transaction {
    fn new(install: &Path, backup: &Path) -> io::Result<Self> {
        if backup.exists() {
            clear_directory(backup)?;
        }
        fs::create_dir(backup)?;
        plain(install)?;
        Ok(Self {
            install: install.to_owned(),
            backup: backup.to_owned(),
            changed: vec![],
        })
    }
    fn replace(&mut self, stage: &Path, info: &BuildInfo) -> io::Result<()> {
        for name in info
            .files
            .keys()
            .map(String::as_str)
            .chain(std::iter::once(INFO))
        {
            let target = destination(&self.install, name)?;
            let existed = target.exists();
            if existed {
                let backup = destination(&self.backup, name)?;
                fs::create_dir_all(backup.parent().unwrap())?;
                fs::copy(&target, backup)?;
            }
            atomic_copy(&destination(stage, name)?, &target)?;
            self.changed.push((name.into(), existed));
        }
        Ok(())
    }
    fn rollback(&self) -> io::Result<()> {
        let mut failed = false;
        for (name, existed) in self.changed.iter().rev() {
            let target = destination(&self.install, name)?;
            let result = if *existed {
                atomic_copy(&destination(&self.backup, name)?, &target)
            } else {
                fs::remove_file(&target)
            };
            failed |= result.is_err();
        }
        if failed {
            Err(invalid("rollback failed; update backup retained"))
        } else {
            Ok(())
        }
    }
}
fn atomic_copy(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target.parent().unwrap())?;
    let temporary = target.with_file_name(format!(
        ".{}.lazybuddy-new",
        target.file_name().unwrap().to_string_lossy()
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        io::copy(&mut fs::File::open(source)?, &mut file)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
fn launch_and_confirm(install: &Path) -> io::Result<()> {
    let mut child = windows::command(&install.join("lazybuddy.exe"))
        .arg("--background")
        .current_dir(install)
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if child.try_wait()?.is_some() {
            return Err(invalid("updated application exited before readiness"));
        }
        let request = lazybuddy_host_control::Request {
            version: 1,
            command: "status".into(),
            args: serde_json::json!({}),
        };
        if lazybuddy_host_control::call_sync(&request).is_ok_and(|reply| {
            reply["ok"] == true
                && reply["data"]["pid"].as_u64() == Some(u64::from(child.id()))
                && reply["data"]["window"].is_array()
        }) {
            let _ = lazybuddy_host_control::call_sync(&lazybuddy_host_control::Request {
                version: 1,
                command: "window.show".into(),
                args: serde_json::json!({}),
            });
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(invalid("updated application did not become ready"))
}
pub fn apply_job(path: &Path) -> io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| invalid("missing update directory"))?
        .canonicalize()?;
    let _lock = windows::lock(&directory.join("apply.lock"))?;
    let job: Job = read_json(path)?;
    let install = job.install.canonicalize()?;
    let source = Source::load(&install.join(SOURCE))?;
    let manifest = verify_manifest(
        &fs::read(directory.join("manifest.json"))?,
        &fs::read(directory.join("manifest.json.sig"))?,
        &source.public_key,
    )?;
    verify_stage(&directory.join("stage"), &manifest)?;
    let _install_lock = windows::lock(&install.join(".lazybuddy-update.lock"))?;
    windows::wait_parent(
        job.parent_pid,
        &install.join("lazybuddy.exe"),
        &directory.join("ready"),
    )?;
    let initialized = verify_stage(&directory.join("stage"), &manifest).and_then(|info| {
        Transaction::new(&install, &directory.join("backup")).map(|transaction| (info, transaction))
    });
    let (info, mut transaction) = match initialized {
        Ok(value) => value,
        Err(error) => {
            let _ = launch_and_confirm(&install);
            return Err(error);
        }
    };
    let result = transaction
        .replace(&directory.join("stage"), &info)
        .and_then(|_| launch_and_confirm(&install));
    if result.is_err() {
        transaction.rollback()?;
    }
    let status = serde_json::json!({"ok":result.is_ok(),"build":manifest.build});
    fs::write(
        directory.parent().unwrap().join("result.json"),
        serde_json::to_vec(&status).map_err(io::Error::other)?,
    )?;
    if result.is_err() {
        let _ = launch_and_confirm(&install);
    }
    if result.is_ok() {
        for name in ["stage", "backup"] {
            let _ = clear_directory(&directory.join(name));
        }
        for name in [
            "package.zip",
            "manifest.json",
            "manifest.json.sig",
            "job.json",
            "ready",
        ] {
            let _ = fs::remove_file(directory.join(name));
        }
    }
    result
}
