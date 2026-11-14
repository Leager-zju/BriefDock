use super::*;
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::os::windows::fs::OpenOptionsExt;

struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!(
                "qa-updater-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = clear_directory(&self.0);
    }
}
#[test]
fn concurrent_fixture_directories_are_distinct() {
    let directories: Vec<_> = (0..32).map(|_| Temporary::new()).collect();
    let unique: std::collections::BTreeSet<_> = directories.iter().map(|item| &item.0).collect();
    assert_eq!(unique.len(), directories.len());
}
fn fixture(directory: &Path, label: &str) -> Manifest {
    fs::create_dir_all(directory).unwrap();
    let mut files = BTreeMap::new();
    for name in REQUIRED {
        fs::write(directory.join(name), format!("{label}:{name}")).unwrap();
        files.insert(name.into(), file_digest(&directory.join(name)).unwrap());
    }
    let build = digest(&serde_json::to_vec(&files).unwrap());
    let info = BuildInfo {
        schema: 1,
        version: "2.0.0".into(),
        build: build.clone(),
        files,
    };
    fs::write(directory.join(INFO), serde_json::to_vec(&info).unwrap()).unwrap();
    Manifest {
        schema: 1,
        target: "windows-x86_64".into(),
        version: "2.0.0".into(),
        build,
        url: "https://example.invalid/app.zip".into(),
        bytes: 1,
        sha256: "0".repeat(64),
        host_sha256: info.files["lazybuddy.exe"].clone(),
    }
}
fn archive(directory: &Path, path: &Path, extra: Option<&str>) -> Manifest {
    let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        writer
            .start_file(
                format!("LazyBuddy/{}", entry.file_name().to_string_lossy()),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(&fs::read(entry.path()).unwrap()).unwrap();
    }
    if let Some(name) = extra {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"untrusted").unwrap();
    }
    writer.finish().unwrap();
    let info: BuildInfo = read_json(&directory.join(INFO)).unwrap();
    Manifest {
        schema: 1,
        target: "windows-x86_64".into(),
        version: info.version,
        build: info.build,
        url: "https://example.invalid/app.zip".into(),
        bytes: fs::metadata(path).unwrap().len(),
        sha256: file_digest(path).unwrap(),
        host_sha256: info.files["lazybuddy.exe"].clone(),
    }
}

#[test]
fn signature_rejects_tampering_wrong_signer_and_unsupported_target() {
    let temp = Temporary::new();
    let mut manifest = fixture(&temp.0.join("stage"), "new");
    let signer = Ed25519KeyPair::from_seed_unchecked(&[11; 32]).unwrap();
    let payload = serde_json::to_vec(&manifest).unwrap();
    let signature = signer.sign(&payload);
    assert!(verify_manifest(&payload, signature.as_ref(), signer.public_key().as_ref()).is_ok());
    let wrong = Ed25519KeyPair::from_seed_unchecked(&[12; 32]).unwrap();
    assert!(verify_manifest(&payload, signature.as_ref(), wrong.public_key().as_ref()).is_err());
    let mut corrupted = payload.clone();
    corrupted[0] ^= 1;
    assert!(verify_manifest(&corrupted, signature.as_ref(), signer.public_key().as_ref()).is_err());
    manifest.target = "macos-aarch64".into();
    let payload = serde_json::to_vec(&manifest).unwrap();
    assert!(
        verify_manifest(
            &payload,
            signer.sign(&payload).as_ref(),
            signer.public_key().as_ref()
        )
        .is_err()
    );
}
#[test]
fn same_version_replaced_build_is_an_update_and_bad_local_marker_is_not_current() {
    let temp = Temporary::new();
    let old = temp.0.join("old");
    let new = temp.0.join("new");
    let first = fixture(&old, "old");
    let second = fixture(&new, "new");
    assert_eq!(first.version, second.version);
    assert_ne!(first.build, second.build);
    assert!(current(&old, &first));
    assert!(!current(&old, &second));
    fs::write(old.join("lazybuddy.exe"), b"changed").unwrap();
    assert!(!current(&old, &first));
}
#[test]
fn verified_archive_unpacks_and_rejects_wrong_hash_traversal_or_user_files() {
    let temp = Temporary::new();
    let source = temp.0.join("source");
    fixture(&source, "new");
    let zip = temp.0.join("app.zip");
    let manifest = archive(&source, &zip, None);
    unpack(&zip, &temp.0.join("valid"), &manifest).unwrap();
    let mut wrong = manifest.clone();
    wrong.sha256 = "f".repeat(64);
    assert!(unpack(&zip, &temp.0.join("wrong"), &wrong).is_err());
    for (index, name) in [
        "LazyBuddy/../escape.txt",
        "LazyBuddy/settings.json",
        "LazyBuddy/a:stream",
    ]
    .iter()
    .enumerate()
    {
        let manifest = archive(&source, &zip, Some(name));
        assert!(unpack(&zip, &temp.0.join(format!("bad-{index}")), &manifest).is_err());
    }
    assert!(!temp.0.join("escape.txt").exists());
}
#[test]
fn cached_files_are_revalidated_before_application() {
    let temp = Temporary::new();
    let stage = temp.0.join("stage");
    let manifest = fixture(&stage, "new");
    verify_stage(&stage, &manifest).unwrap();
    fs::write(stage.join("lazybuddy-updater.exe"), b"changed").unwrap();
    assert!(verify_stage(&stage, &manifest).is_err());
}
#[test]
fn transaction_replaces_owned_files_and_preserves_configuration_and_modules() {
    let temp = Temporary::new();
    let install = temp.0.join("install");
    let stage = temp.0.join("stage");
    fixture(&install, "old");
    let manifest = fixture(&stage, "new");
    let info = verify_stage(&stage, &manifest).unwrap();
    fs::write(install.join("settings.json"), b"user settings").unwrap();
    fs::create_dir(install.join("modules")).unwrap();
    fs::write(install.join("modules/keep"), b"module").unwrap();
    let mut transaction = Transaction::new(&install, &temp.0.join("backup")).unwrap();
    transaction.replace(&stage, &info).unwrap();
    assert!(current(&install, &manifest));
    assert_eq!(
        fs::read(install.join("settings.json")).unwrap(),
        b"user settings"
    );
    assert_eq!(fs::read(install.join("modules/keep")).unwrap(), b"module");
}
#[test]
fn partial_replacement_rolls_back_when_a_later_file_is_locked() {
    let temp = Temporary::new();
    let install = temp.0.join("install");
    let stage = temp.0.join("stage");
    let old = fixture(&install, "old");
    let new = fixture(&stage, "new");
    let info = verify_stage(&stage, &new).unwrap();
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(install.join("module-catalog.json"))
        .unwrap();
    let mut transaction = Transaction::new(&install, &temp.0.join("backup")).unwrap();
    assert!(transaction.replace(&stage, &info).is_err());
    assert!(!transaction.changed.is_empty());
    transaction.rollback().unwrap();
    drop(locked);
    assert!(current(&install, &old));
}
#[test]
fn active_update_cache_is_not_pruned() {
    let temp = Temporary::new();
    let cache = temp.0.join("cache");
    fs::create_dir(&cache).unwrap();
    let directory = cache.join("a".repeat(64));
    fs::create_dir(&directory).unwrap();
    let lock = windows::lock(&directory.join("apply.lock")).unwrap();
    prune(&cache, None);
    assert!(directory.exists());
    drop(lock);
    prune(&cache, None);
    assert!(!directory.exists());
}
