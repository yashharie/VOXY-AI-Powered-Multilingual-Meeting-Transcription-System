# VOXY — Build and Packaging Guide

## Prerequisites

| Tool | Version | Notes |
|---|---|---|
| Node.js | 22.x | Frontend toolchain |
| npm | 10.x | Package manager |
| Rust | 1.77+ | `rustup` managed |
| VS Build Tools 2022 | Latest | C++ workload required for MSVC target |
| WebView2 Runtime | Latest | Bundled via NSIS bootstrapper |

> **Important:** The Rust toolchain is configured for `x86_64-pc-windows-msvc`.
> You **must** install **Visual Studio Build Tools 2022** with the
> **"Desktop development with C++"** workload before `cargo build` will work.
>
> Download: https://visualstudio.microsoft.com/visual-cpp-build-tools/

---

## Development

```bat
npm install
npm run tauri dev
```

This starts the Vite dev server on `http://localhost:1420` with HMR and launches
the Tauri desktop window connected to it.

---

## Production build

```bat
npm run tauri build
```

Output:
- `src-tauri/target/release/voxy.exe` — standalone executable
- `src-tauri/target/release/bundle/nsis/VOXY_0.1.0_x64-setup.exe` — NSIS installer
- `src-tauri/target/release/bundle/msi/VOXY_0.1.0_x64_en-US.msi` — MSI package

---

## Icons

Before building the final installer, generate icons from the master 1024×1024 source:

```bat
npm run tauri icon assets/voxy-icon-1024.png
```

This writes all required sizes to `src-tauri/icons/`.

---

## Logging

Set `RUST_LOG` before running to control log verbosity:

```bat
set RUST_LOG=voxy=debug,info
npm run tauri dev
```

Log output goes to stderr (console). File logging can be enabled in
`%APPDATA%\VOXY\config.json` (created on first run).

---

## Tauri configuration

Main config: `src-tauri/tauri.conf.json`
Capability permissions: `src-tauri/capabilities/default.json`

Do not change these files without Member 2's review (they affect all members).
