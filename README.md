# AI Skillview

A small Windows application for keeping reusable AI skills and their prompts close at hand. Rust and the Windows API provide a native interface with no WebView, JavaScript runtime, or .NET runtime.

## Run

Run `dist/AI Skillview Setup.exe` to install for your Windows account. Administrator rights are not required. Setup adds a Start Menu shortcut and offers optional desktop and **Start with Windows** checkboxes. Windows 10 or 11, x64.

The standalone `dist/AI Skillview.exe` remains available for portable use. Neither version requires an account or a separate runtime.

- Create a skill with a name and a prompt, then save it.
- Select a skill to read or copy its prompt. Edit and delete are available in the detail pane.
- Search the library to find a skill.
- Confirmation cards match the app, with explicit Save, Discard, Delete, and Cancel actions. Escape cancels a confirmation.
- Long prompts have a slim scrollbar that supports wheel scrolling and dragging. Short prompts have no scrollbar.
- The window's minimize button hides it in the system tray. Click or double-click the tray icon to reopen it. The tray icon has no context menu.
- The close button or Alt+F4 exits the application entirely. Unsaved edits require confirmation before being discarded.
- **Start with Windows** in the app header enables or disables startup for the current executable. Startup launches stay hidden in the tray; clicking the tray icon or opening the normal shortcut shows the window.

The app starts with an empty library and does not contact a server. Startup is opt-in; use the installed copy so its executable path stays stable. Windows Startup apps controls can independently disable a registered startup app.

Setup installs into `%LOCALAPPDATA%\Programs\AI Skillview` by default. Updates and uninstall require the app to be closed, so pending edits follow the app's normal save/discard flow. Uninstall through Windows Settings removes the application, its shortcuts, and startup registration while keeping your skill library.

## Local data

Skills are saved after each successful create, edit, or delete in:

```text
%LOCALAPPDATA%\AI Skillview\skills.json
```

The file contains plain-text names and prompts in versioned JSON. Writes use a temporary file and atomic replacement. A malformed or unsupported data file produces an error; it is not silently replaced. Keep a copy of `skills.json` to back up the library, and close the app before restoring or editing that file manually.

## Keyboard

| Shortcut | Action |
| --- | --- |
| Ctrl+N | New skill |
| Ctrl+F | Focus search |
| Ctrl+S | Save the current edit |
| Ctrl+Enter | Copy the selected skill's prompt |
| Escape | Cancel editing |
| Tab / Shift+Tab | Move between controls |
| Alt+F4 | Exit |

## Build

Install the stable Rust MSVC toolchain, Visual Studio Build Tools with **Desktop development with C++**, and a Windows SDK. Build on Windows:

```powershell
.\scripts\build.ps1
```

This builds the optimized executable and copies it to `dist/AI Skillview.exe`. The release executable needs only Windows system libraries. The build-only `embed-resource` crate compiles the application icon, version information, and DPI manifest.

To also build the installer, provide Inno Setup 6.3 or newer and run:

```powershell
.\scripts\package.ps1
```

The script finds a local/installed `ISCC.exe`, or accepts its full path through `INNO_SETUP_COMPILER`. It produces `dist/AI Skillview Setup.exe`. The installer and app are currently unsigned.

Development and validation:

```powershell
cargo run
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --check
```

`resources/create-icon.ps1` rebuilds the multi-resolution icon using Windows drawing APIs.
`resources/create-installer-art.ps1` rebuilds the matching installer artwork.
