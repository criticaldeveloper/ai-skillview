# AI Skillview

1. [x] Build a standalone Rust/Win32 shell with accessible native controls and a restrained two-pane interface.
2. [x] Implement searchable skill CRUD, explicit editing, clipboard copy, and atomic local persistence.
3. [x] Add tray restoration, minimize-to-tray, explicit exit, DPI support, and single-instance behavior.
4. [x] Validate storage, compile/lint the release, exercise the native UI, and package the executable.

Validated: five persistence tests; warning-free Clippy; release build; native UI checks for create/edit/save/reopen, search, Unicode clipboard copy, keyboard navigation, unsaved-change protection, deletion cancellation, minimize, single-instance restoration, and exit. Tray clicks and movement between monitors with different DPI remain manual checks.

## Interface refinement

- [x] Remove the duplicate local-storage labels.
- [x] Use square input/button/list edges, complete fills, and no focus rectangles.
- [x] Fix the prompt label overlap and add an overflow-only scroll thumb with wheel and drag support.
- [x] Replace runtime Windows confirmation/error boxes with matching app cards.
- [x] Finish native UI regression checks and package the revision.

Revision validated: five persistence tests, Clippy, formatting, release build; UI checks for complete textarea borders, long-prompt wheel/drag scrolling, hidden short-prompt scrollbar, right-click without focus outlines, custom confirmation keyboard navigation/Save/Cancel, and delete cancellation. The existing user library's hash was unchanged throughout isolated tests.

## Installation and startup

- [x] Add a per-user installer, Start Menu shortcut, optional desktop shortcut, and clean uninstall that preserves skills.
- [x] Add a Start with Windows preference and silent tray startup.
- [x] Validate registration in an isolated test key, build and check the installer, and package the release.

Installation validation: ten tests pass (five storage, five isolated startup tests), Clippy and formatting pass, application and branded Inno Setup installer compile. Native checks confirm hidden startup, duplicate minimized launches staying hidden, normal launch restoring the same process, startup control visibility, installer AppMutex protection, destination and task options. Installer preview was canceled before installation; no existing skill data or production startup registration changed. Final installation/uninstall and a real sign-in remain user checks. Binaries are unsigned.
