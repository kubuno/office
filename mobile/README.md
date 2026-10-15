# Kubuno Office — mobile apps

The Android and iOS apps of Kubuno Office, written in Rust with Kubuno views (`.kbview`), and laid out like the
desktop apps (`../desktop`): one Cargo workspace (`Cargo.toml`), one folder per role, and — Office being one module
with several apps — three crates per app, named after it (`kubuno-office-<app>-mobile-…`). Today: **Documents**;
the other Office apps (spreadsheets, presentations, …) join the same way.

```
mobile/
├── Cargo.toml                                          the mobile Cargo workspace
├── common/kubuno-office-documents-mobile-common/       Documents, complete and portable
│   ├── src/views/*.kbview                              its screens, edited in Visual Studio
│   └── examples/render_views.rs                        render the screens to PNG, no device needed
├── android/kubuno-office-documents-mobile-android/     its APK's entry point (com.kubuno.documents, launcher icon)
└── ios/kubuno-office-documents-mobile-ios/             its iOS app's entry point
```

The screens are rendered by the **shared Kubuno mobile renderer** and run by the **shared Android / iOS shells** —
both in the core repository (`core/mobile`), common to every module's mobile apps. This repository holds the Office
apps only.

In Visual Studio the projects are in the module's solution, under `Mobile/Common`, `Mobile/Android` and
`Mobile/iOS`.

## Migration status

This app is the Visual Studio (Kubuno views) migration of the module's Kotlin/Jetpack Compose app, which lived in
`mobile/` before this layout and stays in the git history as the functional reference. The screens are converted;
the behaviour (data, server calls, offline, notifications…) is still to be migrated from it.

## Build

The core repository is expected next to this one (`../../core`), until core publishes a mobile tag.

```bash
# Portable app: tests, and the screens rendered to PNG files (any OS).
cargo test -p kubuno-office-documents-mobile-common
cargo run  -p kubuno-office-documents-mobile-common --example render_views -- <font.ttf> <out-dir>

# Android APK (needs the Android SDK + NDK and cargo-apk).
cd android/kubuno-office-documents-mobile-android && cargo apk build --target x86_64-linux-android

# iOS (on a Mac with the iOS SDK).
cargo build -p kubuno-office-documents-mobile-ios --target aarch64-apple-ios-sim
```
