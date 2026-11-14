use std::{env, fs, path::PathBuf, process::Command};
fn main() {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    for name in ["lazybuddy.ico", "lazybuddy.manifest"] {
        println!("cargo:rerun-if-changed={}", assets.join(name).display());
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let catalog = assets.parent().unwrap().join("module-catalog.json");
    println!("cargo:rerun-if-changed={}", catalog.display());
    fs::copy(
        catalog,
        output
            .ancestors()
            .nth(3)
            .unwrap()
            .join("module-catalog.json"),
    )
    .expect("copy trusted catalog configuration beside host");
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let numeric = version.replace('.', ",") + ",0";
    let definition = format!(
        r#"1 ICON "lazybuddy.ico"
1 24 "lazybuddy.manifest"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904B0"
  BEGIN
   VALUE "FileDescription", "LazyBuddy Desktop\0"
   VALUE "FileVersion", "{version}\0"
   VALUE "ProductName", "LazyBuddy\0"
   VALUE "ProductVersion", "{version}\0"
   VALUE "OriginalFilename", "lazybuddy.exe\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x0409, 1200
 END
END
"#
    );
    let rc = output.join("lazybuddy.rc");
    fs::write(&rc, definition).unwrap();
    let resource = output.join("lazybuddy.res");
    let status = Command::new(resource_compiler().expect("Windows SDK rc.exe is required"))
        .current_dir(assets)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg(rc)
        .status()
        .expect("start resource compiler");
    assert!(status.success(), "Windows resource compilation failed");
    println!("cargo:rustc-link-arg-bin=lazybuddy={}", resource.display());
}
fn resource_compiler() -> Option<PathBuf> {
    let target = env::var("CARGO_CFG_TARGET_ARCH").ok()?;
    let sdk = find_msvc_tools::find_windows_sdk(&target)?;
    sdk.path()
        .map(|directory| directory.join("rc.exe"))
        .find(|path| path.is_file())
}
