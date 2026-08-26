# Cross-Platform Backends (Phase 4)

This document tracks planned native backends for capture/OCR and UI automation outside Windows.
Current v4 release implements **Windows-first** builtins; other platforms return explicit execution errors.

## Capture & OCR (`capture.rs`)

| Action | Windows (shipped) | macOS (planned) | Linux (planned) |
|--------|-------------------|-----------------|-----------------|
| `capture.screenshot` | `screenshots` crate (GDI) | `screencapturekit` or `core-graphics` | `xcap` or `grim`/`scrot` wrapper |
| `capture.monitors` | `screenshots::Screen::all` | `NSScreen` enumeration | `xrandr` / `wlr-randr` |
| `capture.ocr` | `Windows.Media.Ocr` | Apple **Vision** (`VNRecognizeTextRequest`) | **Tesseract** via `tesseract-rs` |

### Feature split (proposal)

- Keep a single `act-capture` feature; gate OS deps with `cfg(windows|target_os = "macos"|target_os = "linux")`.
- Optional `act-capture-ocr-tesseract` for Linux only (large native dependency).
- Do **not** add a separate `ocr.rs` module — all OCR stays in `capture.rs`.

### macOS notes

- Vision framework requires entitlements for screen capture when reading from display (not file).
- OCR from file path: load `NSImage` → `CGImage` → Vision request; language via `recognitionLanguages`.

### Linux notes

- Wayland vs X11: screenshot backend must be selected at runtime or compile time.
- Tesseract language packs are a deployment concern (document in enterprise-deploy.md).

## UI automation (`ui.rs`)

| Action | Windows (shipped) | macOS (planned) | Linux |
|--------|-------------------|-----------------|-------|
| `ui.window.*` | Win32 `EnumWindows` | Accessibility `AXUIElement` | **Not committed** |
| `ui.window.*` / `ui.element.*` / `ui.click|type|key` | SendInput + UIA | AX API + `CGEvent` | AT-SPI (experimental) |

### Permissions

- Windows: `PermissionKind::Ui` + `PermissionKind::Capture` for OCR/screenshot.
- macOS: requires Accessibility + Screen Recording prompts (user must grant in System Settings).
- Linux: X11 only for low-level input; Wayland generally blocks synthetic input.

## WASM / plugins

Complex, app-specific flows (enterprise IM, custom selectors) should remain in `plugins/` WASM hosts.
Builtins stay small composable blocks (`shell.run` → `ui.*` chains).

## Validation status

| Platform | CI | Manual |
|----------|----|--------|
| Windows | `build-and-test.yml` (`windows-latest`) | OCR/UI on physical machine recommended |
| macOS | not in CI yet | Vision/AX spike needed |
| Linux | not in CI yet | Tesseract + xcap spike needed |
