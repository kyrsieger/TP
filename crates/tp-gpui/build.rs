fn main() {
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_windows_identity();
    }
}

#[cfg(windows)]
fn embed_windows_identity() {
    use std::{env, fs, path::PathBuf};

    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let icon = root.join("../../packaging/icons/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed=build.rs");
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let components: Vec<u16> = ["MAJOR", "MINOR", "PATCH"]
        .map(|part| {
            env::var(format!("CARGO_PKG_VERSION_{part}"))
                .unwrap()
                .parse()
                .expect("Windows version components must fit in 16 bits")
        })
        .into();
    let numeric = format!("{},{},{},0", components[0], components[1], components[2]);
    let icon = icon
        .to_str()
        .expect("Icon path must be UTF-8")
        .replace('\\', "/");
    // GPUI supplies the DPI/compatibility manifest. This resource adds only TP's
    // identity, using the same icon as the original installer and application.
    let source = format!(
        r#"
#pragma code_page(65001)
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "kyrsieger\0"
      VALUE "FileDescription", "TP\0"
      VALUE "FileVersion", "{version}\0"
      VALUE "InternalName", "TP\0"
      VALUE "LegalCopyright", "Copyright (c) 2026 kyrsieger\0"
      VALUE "OriginalFilename", "tp-gpui.exe\0"
      VALUE "ProductName", "TP\0"
      VALUE "ProductVersion", "{version}\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#
    );
    let rc = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("tp-identity.rc");
    fs::write(&rc, source).expect("Write Windows identity resource");
    embed_resource::compile_for(&rc, ["tp-gpui"], embed_resource::NONE)
        .manifest_required()
        .expect("Embed TP icon and version information");
}
