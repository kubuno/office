# Kubuno Documents for Windows

`kubuno-documents.exe`: the app of [`../common`](../common/README.md) with the Windows platform registered. This
folder holds ONLY what Windows does differently:

| Crate | Role |
|---|---|
| `kubuno-office-desktop` | The Win32 window written like a Windows Forms application (`.kbview` views, declarative ribbon, Backstage), the custom controls (page canvas, rulers, zoom slider), DirectWrite measuring and Direct2D painting of the pages, the clipboard, the IME, WIC images and file dialogs, the Windows implementations of the extension points (`src/platform`), and the entry point (`src/main.rs`, manifest and icon) |

The library is what the Visual Studio designer links to render the views (`Kubuno.Office.Desktop.rsproj`).

Build and run: see [`../README.md`](../README.md#build). What stays here and why:
[`../README.md`](../README.md#what-stays-in-windows-and-why).
