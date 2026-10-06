# Kubuno Documents — desktop app

The native desktop word processor of the Office module, one Cargo workspace for every operating system
(`Cargo.toml` here), independent of the server's (`../server`). It opens, edits and saves the module's documents
(the stored ProseMirror JSON, kept losslessly) with the same layout and pagination as the web editor.

```
desktop/
  common/    the COMPLETE app, portable: launch options, the open document's view state and page placement,
             the server session (routes, save rules, crash journal, live thread), start-up, and the platform
             extension points
  windows/   ONLY what Windows does differently: the Win32 window painted with Direct2D, and
             kubuno-documents.exe, the entry point that registers it
  linux/     the Linux entry point: the common app with the portable platform
  macos/     the macOS entry point: the common app with the portable platform
```

| Crate | Folder | Role |
|---|---|---|
| `kubuno-office-desktop-common` | `common/` | The app: `app::Options` and the document they open, `app::run`, the text interface `app::TextUi`, `model::state` (view state, page placement), `api` (`client`: the routes; `session`: when to call them and every rule that protects the user's work; `remote`: the crash journal; `live`: the session thread of an open window), and `platform` (the extension points) |
| `kubuno-office-desktop` | `windows/` | `kubuno-documents.exe`: the window (`.kbview` views, declarative ribbon, Backstage), the custom controls (page canvas, rulers, zoom slider), DirectWrite measuring and Direct2D painting of the pages, clipboard, IME, WIC images, file dialogs, and the Windows implementations of the extension points |
| `kubuno-office-desktop-linux`, `kubuno-office-desktop-macos` | `linux/`, `macos/` | `kubuno-documents`: the app with the portable defaults |

The document engine itself — the stored document, the layout and pagination engine ported from the web editor's
`canvas-engine.ts`, the caret geometry and the editing operations with exact undo — is `kubuno-office-docs-core`
(`../common/core`), the module's shared core, platform-neutral and built for every target (wasm32 included).

## Platform extension points

Everything is written once in `common/`; what only an operating system can do goes through the traits of
`kubuno_office_desktop_common::platform`, each with a portable default, so the common app builds and runs anywhere
with nothing registered. An OS folder overrides only what it does better and registers it in its entry point:

```rust
// windows/kubuno-office-desktop/src/main.rs
fn main() {
    let code = kubuno_office_desktop_common::app::run(
        kubuno_office_desktop::platform::platform(),   // LaunchRules for Windows
        &kubuno_office_desktop::WindowsUi,             // the Win32 window
    );
    std::process::exit(code);
}
```

| Trait | Portable default | Windows override |
|---|---|---|
| `LaunchRules` | `--sample` shows the offline sample header | also a Debug build under a debugger without `--live` |
| `UiHost` | `app::TextUi` (the document as text, paginated by the engine with fixed metrics) | `WindowsUi` (the Win32 window, Direct2D) |

`app::run` registers the platform, reads the command line and, for a server document (`--doc <id>`), borrows the
access tokens from the Kubuno shell's token broker (`kubuno-desktop-sync`, portable): Documents never holds a
password or a refresh token.

## What stays in `windows/`, and why

| What | Why it is Windows-bound today | Way out |
|---|---|---|
| The window, its views and controls (`views/`, `pages/`, `controls/`) | Drawn with the Kubuno desktop framework (`kubuno-desktop`), which paints with Direct2D/DirectWrite into a Win32 window | A portable rendering backend in the framework (core repository, `desktop/`), then the views move to `common/` and each OS supplies a `UiHost` |
| Text measuring and page painting (`doc/fonts.rs`, `doc/paint.rs`, `platform/painter.rs`) | DirectWrite and Direct2D | A `Measure` and a painter extension point here, with the framework's portable backend |
| Clipboard, IME, images, file dialogs (`platform/`) | Win32 clipboard formats, IMM, WIC, `IFileOpenDialog` | Extension points with portable defaults (in-process clipboard, no IME placement, the image bytes as stored, no dialog) when a second native interface needs them |

## Build

The Kubuno desktop framework and the common crates of Kubuno Desktop come from the core repository at the tag named
in `Cargo.toml` (`[workspace.dependencies]`, one tag for all of them, pinned by `Cargo.lock`) and are linked
statically: `kubuno-documents.exe` needs no DLL beside it. From this folder:

```powershell
# Windows (Rust stable, MSVC toolchain)
cargo build --release -p kubuno-office-desktop
target\release\kubuno-documents.exe [content.json | --doc <id>] [--sample] [--dark] [--culture fr|en] [--zoom 50]
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

```sh
# Linux or macOS: the portable app and the text interface (built natively: its HTTP and SQLite dependencies
# compile C, so a check for another target needs that target's C compiler)
cargo test -p kubuno-office-desktop-common
cargo run -p kubuno-office-desktop-linux -- --sample      # or -macos
```

Under `KUBUNO_SANDBOX_DIR` (a directory) a run never writes outside it: the crash journal, the logs and the token
broker it talks to are the sandbox's.

In Visual Studio, the crates are in the repository's solution, `Kubuno.Office.slnx`, under **Desktop** (Common,
Windows, Linux, macOS), and the engine under **Common**.

## History

The app was developed in `kubuno/desktop` (`windows/src/documents`), merged into the core repository
(`desktop/windows/kubuno-office-desktop`) on 2026-10-06, and moved here with its history the same day, with its
engine (`desktop/common/kubuno-office-docs-core`, formerly `kubuno-docs-core`, now `../common/core`), when every
module repository started to hold all its clients; it was then split into the portable `common/` app and the
Windows overrides.
