fn main() {
    // DPI awareness + common controls v6, like the other Kubuno desktop apps.
    embed_manifest::embed_manifest(embed_manifest::new_manifest("Kubuno.Diagrams"))
        .expect("unable to embed application manifest");
    // The app icon, built from the module's own web logo (core/web/public/office-diagrams-logo.png).
    let mut res = winresource::WindowsResource::new();
    res.set_icon_with_id("assets/diagrams.ico", "app_icon");
    res.compile().expect("unable to embed icon resource");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/diagrams.ico");
}
