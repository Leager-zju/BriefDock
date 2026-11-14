//! Authenticate the catalog before trusting its package hash or executable.
use crate::{Manager, invalid};
use lazybuddy_module_contract::{Dependency, Manifest, package_path, valid_id};
use ring::signature::{ED25519, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::Path,
    time::Duration,
};

fn download_agent(timeout: Duration) -> ureq::Agent {
    // Enabling native-tls does not change ureq's Rustls default provider.
    ureq::Agent::config_builder()
        .https_only(true)
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                .build(),
        )
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .timeout_global(Some(timeout))
        .build()
        .new_agent()
}

fn download_error(resource: &str, error: ureq::Error) -> io::Error {
    // Do not expose redirect URLs, proxy credentials or arbitrary TLS error text.
    let reason = match error {
        ureq::Error::StatusCode(code) => format!("HTTP {code}"),
        ureq::Error::Timeout(_) => "network timeout".into(),
        ureq::Error::HostNotFound => "host not found".into(),
        ureq::Error::ConnectionFailed => "connection failed".into(),
        ureq::Error::ConnectProxyFailed(_) => "proxy connection failed".into(),
        _ => "network or TLS error".into(),
    };
    invalid(format!("{resource} download failed: {reason}"))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema: u32,
    pub packages: Vec<Package>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: lazybuddy_module_contract::Kind,
    pub version: String,
    pub target: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub url: String,
    pub public_key: Vec<u8>,
}
impl Source {
    pub fn load(path: &Path) -> io::Result<Self> {
        if fs::metadata(path)?.len() > 4096 {
            return Err(invalid("catalog source too large"));
        }
        let source: Self = serde_json::from_slice(&fs::read(path)?)
            .map_err(|_| invalid("invalid catalog source"))?;
        if !source.url.starts_with("https://") || source.public_key.len() != 32 {
            return Err(invalid("invalid trusted source"));
        }
        Ok(source)
    }
}

impl Catalog {
    pub fn installation_order(&self, id: &str) -> io::Result<Vec<&Package>> {
        fn visit<'a>(
            catalog: &'a Catalog,
            id: &str,
            visiting: &mut Vec<String>,
            result: &mut Vec<&'a Package>,
        ) -> io::Result<()> {
            if result.iter().any(|package| package.id == id) {
                return Ok(());
            }
            if visiting.iter().any(|current| current == id) {
                return Err(invalid("cyclic package dependency"));
            }
            let package = catalog
                .packages
                .iter()
                .find(|package| package.id == id)
                .ok_or_else(|| invalid("dependency missing from catalog"))?;
            visiting.push(id.to_owned());
            for dependency in &package.dependencies {
                let provider = catalog
                    .packages
                    .iter()
                    .find(|package| package.id == dependency.id)
                    .ok_or_else(|| invalid("dependency missing from catalog"))?;
                if provider.version != dependency.version {
                    return Err(invalid("catalog dependency version mismatch"));
                }
                visit(catalog, &dependency.id, visiting, result)?;
            }
            visiting.pop();
            result.push(package);
            Ok(())
        }
        let mut result = Vec::new();
        visit(self, id, &mut Vec::new(), &mut result)?;
        Ok(result)
    }
}

pub fn download_catalog(url: &str, public_key: &[u8]) -> io::Result<Catalog> {
    let (bytes, signature) = download_signed_payload(url, public_key)?;
    verify_catalog(&bytes, &signature, public_key)
}

/// Shared authenticated transport for module catalogs and application manifests.
pub fn download_signed_payload(url: &str, public_key: &[u8]) -> io::Result<(Vec<u8>, Vec<u8>)> {
    fn read(url: &str, limit: u64) -> io::Result<Vec<u8>> {
        if !url.starts_with("https://") {
            return Err(invalid("catalog requires HTTPS"));
        }
        let mut response = download_agent(Duration::from_secs(30))
            .get(url)
            .header("Cache-Control", "no-cache")
            .call()
            .map_err(|error| download_error("catalog", error))?;
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(limit + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            return Err(invalid("catalog response too large"));
        }
        Ok(bytes)
    }
    let bytes = read(url, 1024 * 1024)?;
    let signature = read(&format!("{url}.sig"), 64)?;
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(&bytes, &signature)
        .map_err(|_| invalid("signature invalid"))?;
    Ok((bytes, signature))
}

/// Download a bounded archive. Consumers verify its signed digest before unpacking.
pub fn download_archive(url: &str, destination: &Path, bytes: u64) -> io::Result<()> {
    if !url.starts_with("https://") || bytes == 0 || bytes > 1024 * 1024 * 1024 {
        return Err(invalid("invalid archive download"));
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let result = (|| {
        let mut response = download_agent(Duration::from_secs(1800))
            .get(url)
            .call()
            .map_err(|error| download_error("archive", error))?;
        if io::copy(
            &mut response.body_mut().as_reader().take(bytes + 1),
            &mut file,
        )? != bytes
        {
            return Err(invalid("archive download incomplete"));
        }
        file.sync_all()
    })();
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

pub fn verify_catalog(bytes: &[u8], signature: &[u8], public_key: &[u8]) -> io::Result<Catalog> {
    if bytes.len() > 1024 * 1024 {
        return Err(invalid("catalog too large"));
    }
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(bytes, signature)
        .map_err(|_| invalid("catalog signature invalid"))?;
    let catalog: Catalog =
        serde_json::from_slice(bytes).map_err(|_| invalid("catalog JSON invalid"))?;
    if catalog.schema != 1 {
        return Err(invalid("unsupported catalog version"));
    }
    let mut ids = std::collections::BTreeSet::new();
    for package in &catalog.packages {
        if !valid_id(&package.id)
            || !ids.insert(&package.id)
            || package.target != "windows-x86_64"
            || !package.url.starts_with("https://")
            || package.bytes == 0
            || package.bytes > 1024 * 1024 * 1024
            || package.sha256.len() != 64
            || !package.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid("catalog package invalid"));
        }
        catalog.installation_order(&package.id)?;
    }
    Ok(catalog)
}

impl Manager {
    pub fn install_from_catalog(&self, catalog: &Catalog, id: &str) -> io::Result<Manifest> {
        let _lock = self.lock()?;
        let mut staged = Vec::new();
        let result = (|| {
            for package in catalog.installation_order(id)? {
                if self
                    .manifest(&package.id)
                    .is_ok_and(|manifest| manifest.version == package.version)
                {
                    continue;
                }
                staged.push(self.download_staged(package)?);
            }
            if !staged.is_empty() {
                self.activate_many(&staged)?;
            }
            self.manifest(id)
        })();
        for package in staged {
            let _ = fs::remove_dir_all(package.path);
        }
        result
    }

    pub fn download_and_install(&self, package: &Package) -> io::Result<Manifest> {
        let _lock = self.lock()?;
        let staged = self.download_staged(package)?;
        let result = self
            .activate_many(std::slice::from_ref(&staged))
            .map(|mut manifests| manifests.remove(0));
        let _ = fs::remove_dir_all(staged.path);
        result
    }

    fn download_staged(&self, package: &Package) -> io::Result<crate::Staged> {
        if !valid_id(&package.id) || !package.url.starts_with("https://") {
            return Err(invalid("invalid package download"));
        }
        fs::create_dir_all(self.root())?;
        let archive = self.root().join(format!(".download-{}.zip", package.id));
        let result = (|| {
            download_archive(&package.url, &archive, package.bytes)?;
            self.unpack_verified(&archive, package)
        })();
        let _ = fs::remove_file(&archive);
        result
    }

    pub fn install_verified(&self, archive: &Path, package: &Package) -> io::Result<Manifest> {
        let _lock = self.lock()?;
        let staged = self.unpack_verified(archive, package)?;
        let result = self
            .activate_many(std::slice::from_ref(&staged))
            .map(|mut manifests| manifests.remove(0));
        let _ = fs::remove_dir_all(staged.path);
        result
    }

    fn unpack_verified(&self, archive: &Path, package: &Package) -> io::Result<crate::Staged> {
        if !valid_id(&package.id) || fs::metadata(archive)?.len() != package.bytes {
            return Err(invalid("package size mismatch"));
        }
        let mut source = fs::File::open(archive)?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = source.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        if format!("{:x}", hash.finalize()) != package.sha256.to_ascii_lowercase() {
            return Err(invalid("package checksum mismatch"));
        }
        fs::create_dir_all(self.root())?;
        let staging = self.root().join(format!(".unpack-{}", package.id));
        fs::create_dir(&staging)?;
        let result = (|| {
            let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)
                .map_err(|_| invalid("invalid package archive"))?;
            let mut remaining = 2 * 1024 * 1024 * 1024u64;
            for index in 0..zip.len() {
                let entry = zip
                    .by_index(index)
                    .map_err(|_| invalid("invalid archive entry"))?;
                let name = entry.name().trim_end_matches('/');
                if !package_path(name)
                    || entry
                        .unix_mode()
                        .is_some_and(|mode| mode & 0o170000 == 0o120000)
                {
                    return Err(invalid("unsafe archive entry"));
                }
                let destination = staging.join(name);
                if entry.is_dir() {
                    fs::create_dir_all(destination)?;
                    continue;
                }
                remaining = remaining
                    .checked_sub(entry.size())
                    .ok_or_else(|| invalid("expanded package too large"))?;
                fs::create_dir_all(destination.parent().expect("entry parent"))?;
                let expected = entry.size();
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(destination)?;
                if io::copy(&mut entry.take(expected + 1), &mut file)? != expected {
                    return Err(invalid("expanded entry size mismatch"));
                }
            }
            let manifest = super::read_manifest(&staging)?;
            if manifest.id != package.id
                || manifest.kind != package.kind
                || manifest.version != package.version
                || manifest.target != package.target
                || manifest.dependencies.len() != package.dependencies.len()
                || manifest.dependencies.iter().any(|dependency| {
                    !package.dependencies.iter().any(|declared| {
                        declared.id == dependency.id && declared.version == dependency.version
                    })
                })
            {
                return Err(invalid("catalog and manifest mismatch"));
            }
            Ok(crate::Staged {
                path: staging.clone(),
                manifest,
            })
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    #[test]
    fn existing_archive_target_is_preserved_without_network() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!(
                "qa-download-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(&directory).unwrap();
        let archive = directory.join("existing.zip");
        fs::write(&archive, b"keep").unwrap();
        assert!(download_archive("https://example.invalid/app.zip", &archive, 4).is_err());
        assert_eq!(fs::read(&archive).unwrap(), b"keep");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn download_failures_identify_package_without_leaking_proxy_details() {
        assert_eq!(
            download_error("media-runtime", ureq::Error::StatusCode(503)).to_string(),
            "media-runtime download failed: HTTP 503"
        );
        let error = download_error(
            "hongguo",
            ureq::Error::ConnectProxyFailed("http://user:secret@proxy/?signed=private".into()),
        )
        .to_string();
        assert_eq!(error, "hongguo download failed: proxy connection failed");
    }

    #[test]
    fn https_with_untrusted_peer_returns_error_without_panicking() {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            while std::time::Instant::now() < deadline {
                if let Ok((mut stream, _)) = listener.accept() {
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\nnot TLS");
                    return true;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            false
        });
        assert!(
            download_agent(Duration::from_secs(2))
                .get(format!("https://{address}/catalog.json"))
                .config()
                .proxy(None)
                .build()
                .call()
                .is_err()
        );
        assert!(
            server.join().unwrap(),
            "HTTPS handshake must reach test peer"
        );
    }

    #[test]
    fn catalog_authentication_rejects_tampering_and_wrong_signer() {
        let signer = Ed25519KeyPair::from_seed_unchecked(&[7u8; 32]).unwrap();
        let data = br#"{"schema":1,"packages":[]}"#;
        let signature = signer.sign(data);
        assert!(verify_catalog(data, signature.as_ref(), signer.public_key().as_ref()).is_ok());
        assert!(
            verify_catalog(
                br#"{"schema":2,"packages":[]}"#,
                signature.as_ref(),
                signer.public_key().as_ref()
            )
            .is_err()
        );
        let other = Ed25519KeyPair::from_seed_unchecked(&[8u8; 32]).unwrap();
        assert!(verify_catalog(data, signature.as_ref(), other.public_key().as_ref()).is_err());
    }

    #[test]
    fn dependencies_are_installed_first_and_cycles_are_rejected() {
        let mut catalog: Catalog = serde_json::from_value(serde_json::json!({"schema":1,"packages":[
            {"id":"feature","version":"1.0.0","target":"windows-x86_64","url":"https://example.org/f.zip","sha256":"0".repeat(64),"bytes":1,"dependencies":[{"id":"runtime","version":"1.0.0"}]},
            {"id":"runtime","version":"1.0.0","target":"windows-x86_64","url":"https://example.org/r.zip","sha256":"0".repeat(64),"bytes":1}
        ]})).unwrap();
        assert_eq!(
            catalog
                .installation_order("feature")
                .unwrap()
                .iter()
                .map(|package| package.id.as_str())
                .collect::<Vec<_>>(),
            vec!["runtime", "feature"]
        );
        catalog.packages[1].dependencies.push(Dependency {
            id: "feature".into(),
            version: "1.0.0".into(),
        });
        assert!(catalog.installation_order("feature").is_err());
        catalog.packages[1].dependencies.clear();
        catalog.packages[0].dependencies[0].version = "2.0.0".into();
        assert!(catalog.installation_order("feature").is_err());
    }

    #[test]
    fn verified_archives_reject_unsafe_paths_and_clean_staging() {
        use std::io::Write;
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!(
                "qa-package-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(&directory).unwrap();
        let manager = Manager::new(directory.join("modules"));
        for unsafe_name in ["../outside", "bin/service:stream"] {
            let archive = directory.join("fixture.zip");
            let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
            zip.start_file(unsafe_name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"fixture").unwrap();
            zip.finish().unwrap();
            let bytes = fs::read(&archive).unwrap();
            let package = Package {
                id: "sample".into(),
                name: "sample".into(),
                kind: lazybuddy_module_contract::Kind::Feature,
                version: "0.1.0".into(),
                target: "windows-x86_64".into(),
                url: "https://example.org/news.zip".into(),
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                bytes: bytes.len() as u64,
                dependencies: vec![],
            };
            assert!(manager.install_verified(&archive, &package).is_err());
            assert!(!manager.root().join(".unpack-news").exists());
            assert!(!directory.join("outside").exists());
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn archive_identity_and_digest_are_checked_before_activation() {
        use std::io::Write;
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!(
                "qa-package-valid-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(&directory).unwrap();
        let manager = Manager::new(directory.join("modules"));
        let archive = directory.join("fixture.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        zip.start_file("module.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(include_bytes!("../../../contracts/fixtures/feature.json"))
            .unwrap();
        zip.start_file("bin/service.exe", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"fixture; never executed").unwrap();
        zip.finish().unwrap();
        let bytes = fs::read(&archive).unwrap();
        let mut package = Package {
            id: "sample".into(),
            name: "sample".into(),
            kind: lazybuddy_module_contract::Kind::Feature,
            version: "0.2.0".into(),
            target: "windows-x86_64".into(),
            url: "https://example.org/news.zip".into(),
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            bytes: bytes.len() as u64,
            dependencies: vec![],
        };
        assert!(manager.install_verified(&archive, &package).is_err());
        assert!(manager.list().unwrap().is_empty());
        package.version = "0.1.0".into();
        let digest = package.sha256.clone();
        package.sha256 = "0".repeat(64);
        assert!(manager.install_verified(&archive, &package).is_err());
        package.sha256 = digest;
        assert_eq!(
            manager.install_verified(&archive, &package).unwrap().id,
            "sample"
        );
        assert!(!manager.root().join(".unpack-news").exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
