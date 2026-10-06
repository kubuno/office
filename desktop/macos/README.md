# Kubuno Documents for macOS

`kubuno-office-desktop-macos` (binary `kubuno-documents`): the app of [`../common`](../common/README.md) with the
portable platform, and its text interface (`app::TextUi`, the document paginated by the engine) until the Kubuno
desktop framework renders on macOS. macOS overrides nothing yet: its native window and services will join this folder
as implementations of the extension points of `kubuno_office_desktop_common::platform` (see
[`../README.md`](../README.md#platform-extension-points)).

```sh
cargo run -p kubuno-office-desktop-macos -- --sample      # from desktop/
```
