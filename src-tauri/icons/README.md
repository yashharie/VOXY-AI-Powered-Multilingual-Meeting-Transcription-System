# VOXY — Application Icons

Place the following icon files in this directory before building the final package:

| File | Usage |
|---|---|
| `32x32.png` | Windows taskbar / small icon |
| `128x128.png` | Windows installer / about screen |
| `128x128@2x.png` | HiDPI display |
| `icon.ico` | Windows .exe / shortcut icon (multi-size ICO) |
| `icon.icns` | macOS (not required for the Windows baseline) |

## Generating icons from a source image

Using the Tauri CLI once VS Build Tools are installed:

```bash
npm run tauri icon path/to/voxy-icon-1024.png
```

This command generates all required sizes automatically and places them here.

## Temporary placeholder

For development builds, copy any 32x32, 128x128 PNG and a valid `.ico` file here.
The Tauri bundler will fail if the files listed in `tauri.conf.json > bundle > icon` are missing.
