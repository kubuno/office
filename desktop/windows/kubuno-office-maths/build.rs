fn main() {
    // DPI awareness + common controls v6, like the other Kubuno desktop apps.
    embed_manifest::embed_manifest(embed_manifest::new_manifest("Kubuno.Maths"))
        .expect("unable to embed application manifest");
    // The app icon (a placeholder until the module's web logo, core/web office-maths-logo.png, is exported).
    let mut res = winresource::WindowsResource::new();
    res.set_icon_with_id("assets/maths.ico", "app_icon");
    res.compile().expect("unable to embed icon resource");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/maths.ico");
}
