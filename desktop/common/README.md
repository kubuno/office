# Kubuno Documents desktop — the portable app

The COMPLETE desktop app, for every operating system; the OS folders (`../windows`, `../linux`, `../macos`) only
override its platform extension points and start it.

| Crate | Role |
|---|---|
| `kubuno-office-desktop-common` | Launch options and the document they open (`app::Options`), start-up (`app::run`), the text interface (`app::TextUi`), the open document's view state and page placement (`model::state`), the server session (`api`: routes, save rules, crash journal, live thread) and the extension points (`platform`: `LaunchRules`, `UiHost`) |

No `windows` crate and no UI framework here: it builds and its tests pass on Windows, Linux and macOS (the CI
builds it natively on each; its HTTP and SQLite dependencies compile C, so a cross check needs the target's C
compiler). The document engine is `../../common/core` (`kubuno-office-docs-core`).
