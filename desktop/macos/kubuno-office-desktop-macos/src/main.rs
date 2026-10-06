//! Kubuno Documents for macOS: the app of desktop/common with the portable platform. macOS overrides nothing yet;
//! a native window and the macOS services join here as implementations of the extension points of
//! `kubuno_office_desktop_common::platform` (see desktop/README.md, "Platform extension points").

use kubuno_office_desktop_common::{app, platform::Platform};

fn main() {
    let mut platform = Platform::portable();
    platform.name = "macos";
    std::process::exit(app::run(platform, &app::TextUi));
}
