fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/icon.png");
    println!("cargo:rerun-if-changed=build.rs");

    // Icon nur für Windows-Target einbetten – funktioniert auch beim Cross-Compile von Linux
    // CARGO_CFG_TARGET_OS ist "windows" wenn --target x86_64-pc-windows-gnu/msvc aktiv ist
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target = std::env::var("TARGET").unwrap_or_default();
    let target_env_check = target_os == "windows" || target.contains("windows");

    // Fallback für native Windows Builds (cfg)
    #[cfg(windows)]
    let is_windows_target = true;
    #[cfg(not(windows))]
    let is_windows_target = target_env_check;
    if is_windows_target {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
        let icon_path = format!("{}/assets/icon.ico", manifest_dir);
        if std::path::Path::new(&icon_path).exists() {
            let mut res = winres::WindowsResource::new();
            res.set_icon(&icon_path);
            // Vollständiges Manifest mit asInvoker + compatibility (verhindert SmartScreen "Herausgeber nicht verifiziert")
            // Ohne compatibility wird die EXE von Windows als legacy eingestuft und als nicht verifiziert angezeigt.
            res.set_manifest(r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity version="0.1.0.0" processorArchitecture="*" name="gitmanager" type="win32"/>
  <description>gitmanager - Git Repository Manager</description>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{e2011457-1546-43c5-a5fe-008deee3d3f0}"/>
      <supportedOS Id="{35138b9a-5d96-4fbd-8e2d-a2440225f93a}"/>
      <supportedOS Id="{4a2f28e3-53b9-4441-ba9c-d69d4a4a6e38}"/>
      <supportedOS Id="{1f676c76-80e1-4239-95bb-83d0f6d0da78}"/>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#);
            if let Err(e) = res.compile() {
                eprintln!("winres Fehler (Icon wird nicht eingebettet): {e}");
                println!("cargo:warning=winres Fehler: {e}");
                // Nicht fehlschlagen – Build soll trotzdem durchgehen
            } else {
                println!("cargo:rerun-if-changed=assets/icon.ico");
                // Direktes Einbetten des resource.o erzwingen – static lib wird sonst vom Linker verworfen
                // da .rsrc keine Symbole enthält. Objekt-Dateien werden immer gelinkt.
                if let Ok(out_dir) = std::env::var("OUT_DIR") {
                    let resource_o = format!("{}/resource.o", out_dir);
                    // Auch als static lib mit whole-archive als Fallback
                    println!("cargo:rustc-link-arg=-Wl,--whole-archive");
                    println!("cargo:rustc-link-search=native={}", out_dir);
                    println!("cargo:rustc-link-lib=static=resource");
                    println!("cargo:rustc-link-arg=-Wl,--no-whole-archive");
                    // Direktes Objekt als sicherer Fallback (wird immer gelinkt)
                    if std::path::Path::new(&resource_o).exists() {
                        println!("cargo:rustc-link-arg={}", resource_o);
                    }
                }
            }
        } else {
            eprintln!("Warnung: Icon nicht gefunden: {}", icon_path);
            println!("cargo:warning=Icon nicht gefunden: {}", icon_path);
        }
    }
}
