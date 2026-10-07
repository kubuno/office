fn main() {
    // DPI awareness + common controls v6, like the other Kubuno desktop apps.
    embed_manifest::embed_manifest(embed_manifest::new_manifest("Kubuno.Spreadsheets"))
        .expect("unable to embed application manifest");
    // The app icon, under the name the window class loads it by: the module's own web logo
    // (core/web/public/office-spreadsheets-logo.png), never the generic Kubuno mark.
    let mut res = winresource::WindowsResource::new();
    res.set_icon_with_id("assets/spreadsheets.ico", "app_icon");
    res.compile().expect("unable to embed icon resource");
    // A deep chain of formulas recurses through the evaluator: give the main thread room (the engine
    // stops at its own depth limit, `formula::eval::MAX_DEPTH`, long before this is used up).
    println!("cargo:rustc-link-arg-bins=/STACK:67108864");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/spreadsheets.ico");
}
