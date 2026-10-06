//! Kubuno Documents for Linux: the app of desktop/common with the portable platform. Linux overrides nothing yet;
//! a native window and the Linux services join here as implementations of the extension points of
//! `kubuno_office_desktop_common::platform` (see desktop/README.md, "Platform extension points").

use kubuno_office_desktop_common::{app, platform::Platform};

fn main() {
    let mut platform = Platform::portable();
    platform.name = "linux";
    std::process::exit(app::run(platform, &app::TextUi));
}
