fn main() {
    // DPI awareness + common controls v6, like the other Kubuno desktop apps.
    embed_manifest::embed_manifest(embed_manifest::new_manifest("Kubuno.Whiteboard")).expect("unable to embed application manifest");
    // The app icon (a placeholder until the module's own web logo is exported as a .ico).
    let mut res = winresource::WindowsResource::new();
    res.set_icon_with_id("assets/whiteboard.ico", "app_icon");
    res.compile().expect("unable to embed icon resource");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/whiteboard.ico");
}
