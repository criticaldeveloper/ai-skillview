use crate::dialog::{self, DialogKind, DialogResult};
use crate::startup;
use crate::storage::{Skill, Store};
use std::cell::RefCell;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::LibraryLoader::*;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::System::WindowsProgramming::MulDiv;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "CriticalDeveloper.AISkillview.Window";
const TITLE: &str = "AI Skillview";
const TRAY_MESSAGE: u32 = WM_APP + 1;
const ACTION: u32 = WM_APP + 2;
const RESTORE: u32 = WM_APP + 3;
const SEARCH: u16 = 101;
const NEW: u16 = 102;
const LIST: u16 = 103;
const EDIT: u16 = 104;
const DELETE: u16 = 105;
const NAME: u16 = 106;
const PROMPT: u16 = 107;
const CANCEL: u16 = 108;
const SAVE: u16 = 109;
const COPY: u16 = 110;
const FIRST: u16 = 111;
const EXIT: u16 = 112;
const AUTOSTART: u16 = 113;
const INK: u32 = rgb(36, 43, 41);
const MUTED: u32 = rgb(108, 113, 109);
const PAPER: u32 = rgb(252, 251, 248);
const SIDEBAR: u32 = rgb(244, 243, 239);
const FIELD: u32 = rgb(255, 255, 253);
const LINE: u32 = rgb(221, 223, 216);
const ACCENT: u32 = rgb(173, 65, 33);
const SELECTED: u32 = rgb(234, 230, 219);
const ERROR: u32 = rgb(171, 49, 39);

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn windows_lines(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\n', "\r\n")
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    View,
    New,
    Edit(u64),
}

#[derive(Default)]
struct Controls {
    startup: HWND,
    search_label: HWND,
    search: HWND,
    new: HWND,
    list: HWND,
    edit: HWND,
    delete: HWND,
    name: HWND,
    prompt: HWND,
    cancel: HWND,
    save: HWND,
    copy: HWND,
    first: HWND,
    name_label: HWND,
    prompt_label: HWND,
}

struct Fonts {
    body: HFONT,
    strong: HFONT,
    small: HFONT,
    title: HFONT,
    brand: HFONT,
}

impl Fonts {
    unsafe fn new(dpi: u32) -> Self {
        unsafe fn font(dpi: u32, pixels: i32, weight: u32) -> HFONT {
            CreateFontW(
                -MulDiv(pixels, dpi as i32, 96),
                0,
                0,
                0,
                weight as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32 | FF_DONTCARE as u32,
                wide("Segoe UI").as_ptr(),
            )
        }
        Self {
            body: font(dpi, 15, FW_NORMAL),
            strong: font(dpi, 15, FW_SEMIBOLD),
            small: font(dpi, 12, FW_NORMAL),
            title: font(dpi, 29, FW_SEMIBOLD),
            brand: font(dpi, 19, FW_SEMIBOLD),
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            for font in [self.body, self.strong, self.small, self.title, self.brand] {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
        }
    }
}

struct App {
    hwnd: HWND,
    instance: HINSTANCE,
    icon: HICON,
    dpi: u32,
    fonts: Fonts,
    controls: Controls,
    focus: HWND,
    store: Store,
    visible: Vec<u64>,
    selected: Option<u64>,
    mode: Mode,
    original: (String, String),
    status: String,
    status_error: bool,
    taskbar_created: u32,
    tray_added: bool,
    scroll_drag: Option<i32>,
    wheel_remainder: i32,
    startup_enabled: bool,
}

struct PromptScroll {
    track: RECT,
    thumb: RECT,
    first: i32,
    max: i32,
}

impl App {
    fn scale(&self, value: i32) -> i32 {
        (value as i64 * self.dpi as i64 / 96) as i32
    }

    unsafe fn client_size(&self) -> (i32, i32) {
        let mut rect = zeroed();
        GetClientRect(self.hwnd, &mut rect);
        (
            MulDiv(rect.right, 96, self.dpi as i32),
            MulDiv(rect.bottom, 96, self.dpi as i32),
        )
    }

    unsafe fn control(&self, class: &str, text: &str, style: u32, id: u16) -> HWND {
        CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | style,
            0,
            0,
            0,
            0,
            self.hwnd,
            id as usize as HMENU,
            self.instance,
            null(),
        )
    }

    unsafe fn button(&self, text: &str, id: u16) -> HWND {
        self.control(
            "BUTTON",
            text,
            WS_TABSTOP | BS_OWNERDRAW as u32 | BS_NOTIFY as u32,
            id,
        )
    }

    unsafe fn init(&mut self) -> Result<(), String> {
        self.dpi = GetDpiForWindow(self.hwnd);
        self.fonts = Fonts::new(self.dpi);
        // Creation order is also the native keyboard and accessibility order.
        self.controls.new = self.button("+  New skill", NEW);
        self.controls.search_label = self.control("STATIC", "Search skills", 0, 122);
        self.controls.search = self.control("EDIT", "", WS_TABSTOP | ES_AUTOHSCROLL as u32, SEARCH);
        self.controls.list = self.control(
            "LISTBOX",
            "Skills",
            WS_TABSTOP
                | WS_VSCROLL
                | LBS_NOTIFY as u32
                | LBS_OWNERDRAWFIXED as u32
                | LBS_HASSTRINGS as u32
                | LBS_NOINTEGRALHEIGHT as u32,
            LIST,
        );
        self.controls.edit = self.button("Edit skill", EDIT);
        self.controls.delete = self.button("Delete", DELETE);
        self.controls.name_label = self.control("STATIC", "Skill name", 0, 120);
        self.controls.name = self.control("EDIT", "", WS_TABSTOP | ES_AUTOHSCROLL as u32, NAME);
        self.controls.prompt_label = self.control("STATIC", "How to use", 0, 121);
        self.controls.prompt = self.control(
            "EDIT",
            "",
            WS_TABSTOP | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32,
            PROMPT,
        );
        if SetWindowSubclass(
            self.controls.prompt,
            Some(prompt_proc),
            1,
            self.hwnd as usize,
        ) == 0
        {
            return Err("Windows could not initialize prompt scrolling.".into());
        }
        self.controls.cancel = self.button("Cancel", CANCEL);
        self.controls.save = self.button("Save skill", SAVE);
        self.controls.copy = self.button("Copy prompt", COPY);
        self.controls.first = self.button("+  Create your first skill", FIRST);
        self.controls.startup = self.button("Start with Windows: Off", AUTOSTART);
        if self.handles().iter().any(|h| h.is_null()) {
            return Err("Windows could not create the application controls.".into());
        }
        SetWindowTheme(self.controls.list, wide("Explorer").as_ptr(), null());
        SendMessageW(
            self.controls.search,
            EM_SETCUEBANNER,
            0,
            wide("Search skills...").as_ptr() as isize,
        );
        SendMessageW(
            self.controls.name,
            EM_SETCUEBANNER,
            0,
            wide("e.g. Review a pull request").as_ptr() as isize,
        );
        SendMessageW(self.controls.name, EM_SETLIMITTEXT, 240, 0);
        SendMessageW(self.controls.search, EM_SETLIMITTEXT, 240, 0);
        SendMessageW(self.controls.prompt, EM_SETLIMITTEXT, 16 * 1024 * 1024, 0);
        SendMessageW(
            self.controls.prompt,
            EM_SETMARGINS,
            EC_LEFTMARGIN as usize | EC_RIGHTMARGIN as usize,
            ((self.scale(3) as u32) | ((self.scale(3) as u32) << 16)) as isize,
        );
        self.apply_fonts();
        match startup::is_enabled() {
            Ok(enabled) => {
                self.startup_enabled = enabled;
                Self::set_text(
                    self.controls.startup,
                    if enabled {
                        "Start with Windows: On"
                    } else {
                        "Start with Windows: Off"
                    },
                );
            }
            Err(_) => {
                EnableWindow(self.controls.startup, 0);
            }
        }
        self.focus = self.controls.new;
        ShowWindow(self.controls.new, SW_SHOW);
        ShowWindow(self.controls.search_label, SW_SHOW);
        ShowWindow(self.controls.search, SW_SHOW);
        ShowWindow(self.controls.startup, SW_SHOW);
        self.selected = self.store.skills().first().map(|s| s.id);
        self.refresh_list();
        self.show_detail();
        self.add_tray();
        Ok(())
    }

    fn handles(&self) -> [HWND; 15] {
        let c = &self.controls;
        [
            c.new,
            c.search_label,
            c.search,
            c.list,
            c.edit,
            c.delete,
            c.name,
            c.prompt,
            c.cancel,
            c.save,
            c.copy,
            c.first,
            c.name_label,
            c.prompt_label,
            c.startup,
        ]
    }

    unsafe fn apply_fonts(&self) {
        for handle in self.handles() {
            SendMessageW(handle, WM_SETFONT, self.fonts.body as usize, 0);
        }
        SendMessageW(
            self.controls.search_label,
            WM_SETFONT,
            self.fonts.small as usize,
            0,
        );
        SendMessageW(
            self.controls.name_label,
            WM_SETFONT,
            self.fonts.small as usize,
            0,
        );
        SendMessageW(
            self.controls.prompt_label,
            WM_SETFONT,
            self.fonts.small as usize,
            0,
        );
        SendMessageW(
            self.controls.list,
            LB_SETITEMHEIGHT,
            0,
            self.scale(76) as isize,
        );
    }

    unsafe fn place(&self, handle: HWND, x: i32, y: i32, width: i32, height: i32) {
        MoveWindow(
            handle,
            self.scale(x),
            self.scale(y),
            self.scale(width.max(0)),
            self.scale(height.max(0)),
            0,
        );
    }

    unsafe fn layout(&self) {
        if self.controls.new.is_null() {
            return;
        }
        let (w, h) = self.client_size();
        let left = 338;
        let right = w - 38;
        let body = right - left;
        let compact = if h < 620 { 36 } else { 0 };
        self.place(self.controls.startup, w - 224, 20, 200, 40);
        self.place(self.controls.new, 22, 128, 254, 42);
        self.place(self.controls.search_label, 24, 184, 250, 18);
        self.place(self.controls.search, 36, 216, 226, 23);
        self.place(self.controls.list, 14, 262, 270, h - 315);
        self.place(self.controls.edit, right - 172, 112, 104, 35);
        self.place(self.controls.delete, right - 60, 112, 60, 35);
        self.place(self.controls.name_label, left, 210 - compact, body, 20);
        self.place(self.controls.name, left + 14, 250 - compact, body - 28, 25);
        let editing = self.mode != Mode::View;
        let prompt_y = if editing {
            333 - compact
        } else {
            252 - compact
        };
        self.place(
            self.controls.prompt_label,
            left,
            if editing {
                302 - compact
            } else {
                216 - compact
            },
            body,
            18,
        );
        self.place(
            self.controls.prompt,
            left + 14,
            prompt_y,
            body - 44,
            h - prompt_y - 132,
        );
        self.place(self.controls.cancel, right - 244, h - 94, 102, 42);
        self.place(self.controls.save, right - 130, h - 94, 130, 42);
        self.place(self.controls.copy, right - 148, h - 94, 148, 42);
        self.place(
            self.controls.first,
            left + (body - 224) / 2,
            h / 2 + 88,
            224,
            44,
        );
        let mut rect = zeroed();
        GetClientRect(self.controls.prompt, &mut rect);
        rect.top = self.scale(2);
        SendMessageW(
            self.controls.prompt,
            EM_SETRECTNP,
            0,
            &mut rect as *mut _ as isize,
        );
        RedrawWindow(
            self.hwnd,
            null(),
            null_mut(),
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
        );
    }

    unsafe fn prompt_bounds(&self) -> [i32; 4] {
        let (w, h) = self.client_size();
        let compact = if h < 620 { 36 } else { 0 };
        [
            338,
            if self.mode == Mode::View { 240 } else { 321 } - compact,
            w - 38,
            h - 117,
        ]
    }

    unsafe fn prompt_scroll(&self) -> Option<PromptScroll> {
        if self.mode == Mode::View && self.skill().is_none() {
            return None;
        }
        let edit = self.controls.prompt;
        let count = SendMessageW(edit, EM_GETLINECOUNT, 0, 0) as i32;
        let mut format: RECT = zeroed();
        SendMessageW(edit, EM_GETRECT, 0, &mut format as *mut _ as isize);
        let dc = GetDC(edit);
        if dc.is_null() {
            return None;
        }
        let font = SelectObject(dc, self.fonts.body);
        let mut metrics: TEXTMETRICW = zeroed();
        GetTextMetricsW(dc, &mut metrics);
        SelectObject(dc, font);
        ReleaseDC(edit, dc);
        let page = ((format.bottom - format.top) / metrics.tmHeight.max(1)).max(1);
        let max = count - page;
        if max <= 0 {
            return None;
        }
        let bounds = self.prompt_bounds();
        let track = RECT {
            left: self.scale(bounds[2] - 17),
            top: self.scale(bounds[1] + 12),
            right: self.scale(bounds[2] - 11),
            bottom: self.scale(bounds[3] - 12),
        };
        let height = track.bottom - track.top;
        if height <= 0 {
            return None;
        }
        let thumb_height = ((height as i64 * page as i64 / count as i64) as i32)
            .max(self.scale(28))
            .min(height);
        let first = (SendMessageW(edit, EM_GETFIRSTVISIBLELINE, 0, 0) as i32).max(0);
        let top = track.top
            + ((height - thumb_height) as i64 * first.min(max) as i64 / max as i64) as i32;
        Some(PromptScroll {
            track,
            thumb: RECT {
                left: track.left,
                top,
                right: track.right,
                bottom: top + thumb_height,
            },
            first,
            max,
        })
    }

    unsafe fn invalidate_scroll(&self) {
        let bounds = self.prompt_bounds();
        let gutter = RECT {
            left: self.scale(bounds[2] - 21),
            top: self.scale(bounds[1] + 2),
            right: self.scale(bounds[2] - 2),
            bottom: self.scale(bounds[3] - 2),
        };
        InvalidateRect(self.hwnd, &gutter, 0);
    }

    unsafe fn drag_scroll(&self, y: i32) {
        let (Some(offset), Some(scroll)) = (self.scroll_drag, self.prompt_scroll()) else {
            return;
        };
        let travel =
            scroll.track.bottom - scroll.track.top - (scroll.thumb.bottom - scroll.thumb.top);
        if travel <= 0 {
            return;
        }
        let position = (y - scroll.track.top - offset).clamp(0, travel);
        let target =
            ((position as i64 * scroll.max as i64 + travel as i64 / 2) / travel as i64) as i32;
        SendMessageW(
            self.controls.prompt,
            EM_LINESCROLL,
            0,
            (target - scroll.first) as isize,
        );
        self.invalidate_scroll();
    }

    unsafe fn wheel_scroll(&mut self, delta: i32) {
        let Some(scroll) = self.prompt_scroll() else {
            self.wheel_remainder = 0;
            return;
        };
        self.wheel_remainder += delta;
        let steps = self.wheel_remainder / WHEEL_DELTA as i32;
        self.wheel_remainder %= WHEEL_DELTA as i32;
        if steps == 0 {
            return;
        }
        let mut lines: u32 = 3;
        SystemParametersInfoW(
            SPI_GETWHEELSCROLLLINES,
            0,
            &mut lines as *mut _ as *mut _,
            0,
        );
        let lines = if lines == u32::MAX {
            SendMessageW(self.controls.prompt, EM_GETLINECOUNT, 0, 0) as i64 - scroll.max as i64
        } else {
            lines as i64
        };
        let target =
            (scroll.first as i64 - steps as i64 * lines).clamp(0, scroll.max as i64) as i32;
        SendMessageW(
            self.controls.prompt,
            EM_LINESCROLL,
            0,
            (target - scroll.first) as isize,
        );
        self.invalidate_scroll();
    }

    unsafe fn text(handle: HWND) -> String {
        let length = GetWindowTextLengthW(handle).max(0) as usize;
        let mut buffer = vec![0u16; length + 1];
        let copied =
            GetWindowTextW(handle, buffer.as_mut_ptr(), buffer.len() as i32).max(0) as usize;
        String::from_utf16_lossy(&buffer[..copied])
    }

    unsafe fn set_text(handle: HWND, text: &str) {
        SetWindowTextW(handle, wide(text).as_ptr());
    }

    fn skill(&self) -> Option<&Skill> {
        self.selected
            .and_then(|id| self.store.skills().iter().find(|s| s.id == id))
    }

    unsafe fn refresh_list(&mut self) {
        let query = Self::text(self.controls.search).trim().to_lowercase();
        let matches: Vec<_> = self
            .store
            .skills()
            .iter()
            .filter(|skill| {
                query.is_empty()
                    || skill.name.to_lowercase().contains(&query)
                    || skill.prompt.to_lowercase().contains(&query)
            })
            .collect();
        self.visible = matches.iter().map(|skill| skill.id).collect();
        SendMessageW(self.controls.list, WM_SETREDRAW, 0, 0);
        SendMessageW(self.controls.list, LB_RESETCONTENT, 0, 0);
        for skill in matches {
            SendMessageW(
                self.controls.list,
                LB_ADDSTRING,
                0,
                wide(&skill.name).as_ptr() as isize,
            );
        }
        self.restore_selection();
        SendMessageW(self.controls.list, WM_SETREDRAW, 1, 0);
        ShowWindow(
            self.controls.list,
            if self.visible.is_empty() {
                SW_HIDE
            } else {
                SW_SHOW
            },
        );
        InvalidateRect(self.controls.list, null(), 1);
        InvalidateRect(self.hwnd, null(), 0);
    }

    unsafe fn restore_selection(&self) {
        let index = self
            .selected
            .and_then(|id| self.visible.iter().position(|v| *v == id));
        SendMessageW(
            self.controls.list,
            LB_SETCURSEL,
            index.map_or(usize::MAX, |v| v),
            0,
        );
    }

    unsafe fn show_detail(&self) {
        let c = &self.controls;
        let editing = self.mode != Mode::View;
        let has_detail = editing || self.skill().is_some();
        for h in [c.edit, c.delete, c.copy] {
            ShowWindow(
                h,
                if has_detail && !editing {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
        }
        for h in [c.name_label, c.name, c.cancel, c.save] {
            ShowWindow(h, if editing { SW_SHOW } else { SW_HIDE });
        }
        for h in [c.prompt_label, c.prompt] {
            ShowWindow(h, if has_detail { SW_SHOW } else { SW_HIDE });
        }
        ShowWindow(c.first, if has_detail { SW_HIDE } else { SW_SHOW });
        SendMessageW(c.prompt, EM_SETREADONLY, (!editing) as usize, 0);
        if !editing {
            Self::set_text(
                c.prompt,
                &self
                    .skill()
                    .map(|s| windows_lines(&s.prompt))
                    .unwrap_or_default(),
            );
        }
        self.layout();
    }

    unsafe fn dirty(&self) -> bool {
        self.mode != Mode::View
            && (
                Self::text(self.controls.name),
                Self::text(self.controls.prompt),
            ) != self.original
    }

    unsafe fn status(&mut self, message: &str, error: bool) {
        self.status = message.to_owned();
        self.status_error = error;
        InvalidateRect(self.hwnd, null(), 0);
    }

    unsafe fn restore_focus(&self) {
        let focus = if IsWindowVisible(self.focus) != 0 && IsWindowEnabled(self.focus) != 0 {
            self.focus
        } else {
            self.controls.new
        };
        if !focus.is_null() {
            SetFocus(focus);
        }
    }

    unsafe fn dialog(&self, title: &str, message: &str, kind: DialogKind) -> DialogResult {
        let result = dialog::show(self.hwnd, title, message, kind);
        self.restore_focus();
        RedrawWindow(
            self.hwnd,
            null(),
            null_mut(),
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
        );
        result
    }

    unsafe fn allow_discard(&mut self) -> bool {
        if !self.dirty() {
            return true;
        }
        match self.dialog(
            "Save your changes?",
            "Save this skill before continuing, or discard the changes you've made.",
            DialogKind::SaveChanges,
        ) {
            DialogResult::Save => self.save(),
            DialogResult::Discard => true,
            _ => false,
        }
    }

    unsafe fn begin_edit(&mut self, new: bool) {
        if !self.allow_discard() {
            return;
        }
        let (mode, name, prompt) = if new {
            (Mode::New, String::new(), String::new())
        } else if let Some(skill) = self.skill() {
            (
                Mode::Edit(skill.id),
                skill.name.clone(),
                windows_lines(&skill.prompt),
            )
        } else {
            return;
        };
        self.mode = mode;
        self.original = (name.clone(), prompt.clone());
        Self::set_text(self.controls.name, &name);
        Self::set_text(self.controls.prompt, &prompt);
        self.status("", false);
        self.show_detail();
        SetFocus(self.controls.name);
        SendMessageW(self.controls.name, EM_SETSEL, 0, -1);
    }

    unsafe fn save(&mut self) -> bool {
        if self.mode == Mode::View {
            return true;
        }
        let name = Self::text(self.controls.name);
        let prompt = Self::text(self.controls.prompt);
        if name.trim().is_empty() || prompt.trim().is_empty() {
            let (message, control) = if name.trim().is_empty() {
                ("Give this skill a name.", self.controls.name)
            } else {
                ("Add a prompt for this skill.", self.controls.prompt)
            };
            self.status(message, true);
            SetFocus(control);
            return false;
        }
        let result = match self.mode {
            Mode::New => self.store.create(&name, &prompt),
            Mode::Edit(id) => self.store.update(id, &name, &prompt).map(|()| id),
            Mode::View => return true,
        };
        match result {
            Ok(id) => {
                self.selected = Some(id);
                self.mode = Mode::View;
                Self::set_text(self.controls.search, "");
                self.refresh_list();
                self.show_detail();
                self.status("Skill saved", false);
                SetFocus(self.controls.copy);
                true
            }
            Err(error) => {
                self.status("Could not save. Your changes are still here.", true);
                self.dialog(
                    "Could not save skill",
                    &error.to_string(),
                    DialogKind::Error,
                );
                false
            }
        }
    }

    unsafe fn delete(&mut self) {
        if self.mode != Mode::View {
            return;
        }
        let Some(skill) = self.skill() else {
            return;
        };
        let id = skill.id;
        let message = format!(
            "Delete \u{201c}{}\u{201d}?\n\nThis permanently removes the skill from this device.",
            skill.name
        );
        if self.dialog("Delete skill", &message, DialogKind::Delete) != DialogResult::Confirm {
            return;
        }
        let old_index = self.visible.iter().position(|v| *v == id).unwrap_or(0);
        match self.store.remove(id) {
            Ok(()) => {
                self.refresh_list();
                self.selected = self
                    .visible
                    .get(old_index.min(self.visible.len().saturating_sub(1)))
                    .copied()
                    .or_else(|| self.store.skills().first().map(|s| s.id));
                self.restore_selection();
                self.show_detail();
                self.status("Skill deleted", false);
                SetFocus(if self.visible.is_empty() {
                    self.controls.new
                } else {
                    self.controls.list
                });
            }
            Err(error) => {
                self.dialog(
                    "Could not delete skill",
                    &error.to_string(),
                    DialogKind::Error,
                );
            }
        }
    }

    unsafe fn copy(&mut self) {
        if self.mode != Mode::View {
            return;
        }
        let Some(skill) = self.skill() else {
            return;
        };
        match copy_to_clipboard(self.hwnd, &skill.prompt) {
            Ok(()) => self.status("Prompt copied to clipboard", false),
            Err(()) => self.status("Clipboard is busy. Try copying again.", true),
        }
    }

    unsafe fn command(&mut self, id: u16, notification: u16) {
        let gained_focus = match id {
            SEARCH | NAME | PROMPT => notification as u32 == EN_SETFOCUS,
            LIST => notification as u32 == LBN_SETFOCUS,
            NEW | FIRST | EDIT | DELETE | COPY | SAVE | CANCEL | AUTOSTART => {
                notification as u32 == BN_SETFOCUS
            }
            _ => false,
        };
        if gained_focus {
            self.focus = GetDlgItem(self.hwnd, id as i32);
            return;
        }
        // Buttons also report focus loss when BS_NOTIFY is enabled.
        if matches!(
            id,
            NEW | FIRST | EDIT | DELETE | COPY | SAVE | CANCEL | AUTOSTART
        ) && notification as u32 != BN_CLICKED
        {
            return;
        }
        match id {
            AUTOSTART => {
                let enabled = !self.startup_enabled;
                match startup::set_enabled(enabled) {
                    Ok(()) => {
                        self.startup_enabled = enabled;
                        Self::set_text(
                            self.controls.startup,
                            if enabled {
                                "Start with Windows: On"
                            } else {
                                "Start with Windows: Off"
                            },
                        );
                        InvalidateRect(self.controls.startup, null(), 1);
                    }
                    Err(error) => {
                        self.dialog(
                            "Could not update startup",
                            &error.to_string(),
                            DialogKind::Error,
                        );
                    }
                }
            }
            NEW | FIRST => self.begin_edit(true),
            EDIT => self.begin_edit(false),
            DELETE => self.delete(),
            COPY => self.copy(),
            SAVE => {
                self.save();
            }
            CANCEL => {
                if self.mode != Mode::View && self.allow_discard() {
                    self.mode = Mode::View;
                    self.status("", false);
                    self.show_detail();
                    SetFocus(self.controls.list);
                }
            }
            EXIT => {
                if self.allow_discard() {
                    DestroyWindow(self.hwnd);
                }
            }
            SEARCH if notification as u32 == EN_CHANGE => self.refresh_list(),
            LIST if notification as u32 == LBN_SELCHANGE => {
                let index = SendMessageW(self.controls.list, LB_GETCURSEL, 0, 0);
                let selected = usize::try_from(index)
                    .ok()
                    .and_then(|i| self.visible.get(i))
                    .copied();
                if selected.is_some() && (selected != self.selected || self.mode != Mode::View) {
                    if self.allow_discard() {
                        self.selected = selected;
                        self.mode = Mode::View;
                        self.status("", false);
                        self.show_detail();
                        self.restore_selection();
                    } else {
                        self.restore_selection();
                    }
                }
            }
            NAME | PROMPT if notification as u32 == EN_CHANGE && self.mode != Mode::View => {
                if !self.status.is_empty() {
                    self.status("", false);
                } else {
                    InvalidateRect(self.hwnd, null(), 0);
                }
            }
            PROMPT if notification as u32 == EN_VSCROLL || notification as u32 == EN_UPDATE => {
                self.invalidate_scroll()
            }
            _ => {}
        }
    }

    unsafe fn add_tray(&mut self) {
        let mut data: NOTIFYICONDATAW = zeroed();
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = 1;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        data.uCallbackMessage = TRAY_MESSAGE;
        data.hIcon = self.icon;
        let tip = wide("AI Skillview \u{00b7} Click to open");
        data.szTip[..tip.len()].copy_from_slice(&tip);
        self.tray_added = Shell_NotifyIconW(NIM_ADD, &data) != 0;
        if self.tray_added {
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            Shell_NotifyIconW(NIM_SETVERSION, &data);
        }
    }

    unsafe fn remove_tray(&self) {
        let mut data: NOTIFYICONDATAW = zeroed();
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = 1;
        Shell_NotifyIconW(NIM_DELETE, &data);
    }

    unsafe fn paint(&self, dc: HDC) {
        let (w, h) = self.client_size();
        self.fill(dc, 0, 0, w, h, PAPER);
        self.fill(dc, 0, 80, 298, h - 114, SIDEBAR);
        self.fill(dc, 0, 79, w, 1, LINE);
        self.fill(dc, 298, 80, 1, h - 114, LINE);
        self.fill(dc, 0, h - 34, w, 1, LINE);
        DrawIconEx(
            dc,
            self.scale(24),
            self.scale(22),
            self.icon,
            self.scale(36),
            self.scale(36),
            0,
            null_mut(),
            DI_NORMAL,
        );
        self.label(
            dc,
            "AI Skillview",
            self.fonts.brand,
            INK,
            [73, 17, 280, 46],
            DT_SINGLELINE,
        );
        self.label(
            dc,
            "Good prompts, always within reach.",
            self.fonts.small,
            MUTED,
            [74, 46, 450, 67],
            DT_SINGLELINE,
        );
        self.label(
            dc,
            "YOUR LIBRARY",
            self.fonts.small,
            MUTED,
            [24, 99, 230, 120],
            DT_SINGLELINE,
        );
        self.label(
            dc,
            &self.store.skills().len().to_string(),
            self.fonts.small,
            MUTED,
            [236, 99, 274, 120],
            DT_RIGHT | DT_SINGLELINE,
        );
        self.panel(dc, [22, 207, 276, 247], FIELD, LINE);
        if self.visible.is_empty() {
            let (title, subtitle) = if self.store.skills().is_empty() {
                (
                    "A little space for big ideas.",
                    "Add your first skill to get started.",
                )
            } else {
                ("No matching skills", "Try a different word or phrase.")
            };
            self.label(
                dc,
                title,
                self.fonts.strong,
                INK,
                [26, 289, 274, 313],
                DT_CENTER | DT_SINGLELINE,
            );
            self.label(
                dc,
                subtitle,
                self.fonts.small,
                MUTED,
                [22, 320, 276, 361],
                DT_CENTER | DT_WORDBREAK,
            );
        }
        let left = 338;
        let right = w - 38;
        let editing = self.mode != Mode::View;
        let compact = if h < 620 { 36 } else { 0 };
        if editing || self.skill().is_some() {
            self.label(
                dc,
                if editing {
                    "MAKE IT USEFUL"
                } else {
                    "READY WHEN YOU ARE"
                },
                self.fonts.small,
                ACCENT,
                [left, 109 - compact / 2, right - 190, 131 - compact / 2],
                DT_SINGLELINE,
            );
            let title = match self.mode {
                Mode::New => "New skill",
                Mode::Edit(_) => "Edit skill",
                Mode::View => self.skill().map_or("", |s| s.name.as_str()),
            };
            self.label(
                dc,
                title,
                self.fonts.title,
                INK,
                [
                    left - 1,
                    147 - compact,
                    if compact > 0 && !editing {
                        right - 190
                    } else {
                        right
                    },
                    190 - compact,
                ],
                DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            if editing {
                self.panel(dc, [left, 239 - compact, right, 285 - compact], FIELD, LINE);
            }
            self.panel(dc, self.prompt_bounds(), FIELD, LINE);
            if let Some(scroll) = self.prompt_scroll() {
                SetDCBrushColor(
                    dc,
                    if self.scroll_drag.is_some() {
                        MUTED
                    } else {
                        rgb(173, 179, 170)
                    },
                );
                FillRect(dc, &scroll.thumb, GetStockObject(DC_BRUSH) as HBRUSH);
            }
            if editing && self.status.is_empty() {
                self.label(
                    dc,
                    "Write the prompt exactly as you want to copy it.",
                    self.fonts.small,
                    MUTED,
                    [left, h - 99, right - 260, h - 50],
                    DT_WORDBREAK,
                );
            } else if !editing && self.status.is_empty() {
                self.label(
                    dc,
                    "A useful idea. One copy away.",
                    self.fonts.small,
                    MUTED,
                    [left, h - 89, right - 168, h - 55],
                    DT_WORDBREAK,
                );
            }
            if !self.status.is_empty() {
                self.label(
                    dc,
                    &self.status,
                    self.fonts.small,
                    if self.status_error { ERROR } else { ACCENT },
                    [
                        left,
                        h - 98,
                        right - if editing { 260 } else { 168 },
                        h - 45,
                    ],
                    DT_WORDBREAK,
                );
            }
        } else {
            let center = (left + right) / 2;
            let y = h / 2 - 112;
            self.round(
                dc,
                [center - 29, y, center + 29, y + 58],
                SELECTED,
                SELECTED,
                14,
            );
            self.label(
                dc,
                "[ ]",
                self.fonts.title,
                ACCENT,
                [center - 29, y + 7, center + 29, y + 51],
                DT_CENTER | DT_SINGLELINE,
            );
            self.label(
                dc,
                "Keep your best prompts.",
                self.fonts.title,
                INK,
                [left, y + 84, right, y + 128],
                DT_CENTER | DT_SINGLELINE,
            );
            self.label(
                dc,
                "Give each skill a name and a prompt.\nFind it, copy it, and put it to work.",
                self.fonts.body,
                MUTED,
                [left, y + 139, right, y + 190],
                DT_CENTER | DT_WORDBREAK,
            );
        }
        self.fill(dc, 24, h - 20, 5, 5, ACCENT);
        self.label(
            dc,
            if editing {
                if self.dirty() {
                    "Unsaved changes"
                } else {
                    "Editing \u{00b7} Save when ready"
                }
            } else {
                "Saved on this device"
            },
            self.fonts.small,
            MUTED,
            [38, h - 27, 335, h - 6],
            DT_SINGLELINE,
        );
        self.label(
            dc,
            if editing {
                "Ctrl+S  Save    /    Esc  Cancel"
            } else {
                "Ctrl+N  New    /    Ctrl+F  Find    /    Ctrl+Enter  Copy"
            },
            self.fonts.small,
            MUTED,
            [340, h - 27, w - 24, h - 6],
            DT_RIGHT | DT_SINGLELINE,
        );
    }

    unsafe fn fill(&self, dc: HDC, x: i32, y: i32, w: i32, h: i32, color: u32) {
        let rect = RECT {
            left: self.scale(x),
            top: self.scale(y),
            right: self.scale(x + w),
            bottom: self.scale(y + h),
        };
        SetDCBrushColor(dc, color);
        FillRect(dc, &rect, GetStockObject(DC_BRUSH) as HBRUSH);
    }

    unsafe fn panel(&self, dc: HDC, rect: [i32; 4], fill: u32, border: u32) {
        let rect = RECT {
            left: self.scale(rect[0]),
            top: self.scale(rect[1]),
            right: self.scale(rect[2]),
            bottom: self.scale(rect[3]),
        };
        paint_box(dc, &rect, fill, border);
    }

    unsafe fn round(&self, dc: HDC, rect: [i32; 4], fill: u32, border: u32, radius: i32) {
        SetDCBrushColor(dc, fill);
        SetDCPenColor(dc, border);
        let brush = SelectObject(dc, GetStockObject(DC_BRUSH));
        let pen = SelectObject(dc, GetStockObject(DC_PEN));
        RoundRect(
            dc,
            self.scale(rect[0]),
            self.scale(rect[1]),
            self.scale(rect[2]),
            self.scale(rect[3]),
            self.scale(radius * 2),
            self.scale(radius * 2),
        );
        SelectObject(dc, brush);
        SelectObject(dc, pen);
    }

    unsafe fn label(
        &self,
        dc: HDC,
        text: &str,
        font: HFONT,
        color: u32,
        rect: [i32; 4],
        flags: u32,
    ) {
        let mut rect = RECT {
            left: self.scale(rect[0]),
            top: self.scale(rect[1]),
            right: self.scale(rect[2]),
            bottom: self.scale(rect[3]),
        };
        draw_text(dc, text, font, color, &mut rect, flags);
    }

    unsafe fn draw_item(&self, item: &DRAWITEMSTRUCT) {
        if item.CtlID == LIST as u32 {
            self.draw_skill(item);
            return;
        }
        let id = item.CtlID as u16;
        if id == AUTOSTART {
            let r = item.rcItem;
            paint_box(item.hDC, &r, PAPER, PAPER);
            let size = self.scale(14);
            let left = self.scale(8);
            let top = (r.bottom - r.top - size) / 2;
            let checkbox = RECT {
                left,
                top,
                right: left + size,
                bottom: top + size,
            };
            paint_box(
                item.hDC,
                &checkbox,
                if self.startup_enabled { ACCENT } else { FIELD },
                if self.startup_enabled { ACCENT } else { LINE },
            );
            if self.startup_enabled {
                let pen = CreatePen(PS_SOLID, self.scale(2).max(1), FIELD);
                let old = SelectObject(item.hDC, pen);
                MoveToEx(
                    item.hDC,
                    left + self.scale(3),
                    top + self.scale(7),
                    null_mut(),
                );
                LineTo(item.hDC, left + self.scale(6), top + self.scale(10));
                LineTo(item.hDC, left + self.scale(12), top + self.scale(3));
                SelectObject(item.hDC, old);
                DeleteObject(pen);
            }
            let mut text_rect = RECT {
                left: self.scale(32),
                ..r
            };
            draw_text(
                item.hDC,
                if item.itemState & ODS_DISABLED != 0 {
                    "Startup unavailable"
                } else {
                    "Start with Windows"
                },
                self.fonts.small,
                MUTED,
                &mut text_rect,
                DT_SINGLELINE | DT_VCENTER,
            );
            return;
        }
        let primary = matches!(id, NEW | SAVE | COPY | FIRST);
        let pressed = item.itemState & ODS_SELECTED != 0;
        let background = if primary {
            if pressed {
                rgb(139, 49, 25)
            } else {
                ACCENT
            }
        } else if pressed {
            SELECTED
        } else {
            PAPER
        };
        let border = if primary {
            background
        } else if id == DELETE {
            PAPER
        } else {
            LINE
        };
        let r = item.rcItem;
        paint_box(item.hDC, &r, background, border);
        let mut rect = r;
        draw_text(
            item.hDC,
            &Self::text(item.hwndItem),
            self.fonts.strong,
            if primary {
                FIELD
            } else if id == DELETE {
                ERROR
            } else {
                INK
            },
            &mut rect,
            DT_SINGLELINE | DT_CENTER | DT_VCENTER,
        );
    }

    unsafe fn draw_skill(&self, item: &DRAWITEMSTRUCT) {
        let r = item.rcItem;
        SetDCBrushColor(item.hDC, SIDEBAR);
        FillRect(item.hDC, &r, GetStockObject(DC_BRUSH) as HBRUSH);
        let Some(id) = self.visible.get(item.itemID as usize) else {
            return;
        };
        let Some(skill) = self.store.skills().iter().find(|s| s.id == *id) else {
            return;
        };
        let selected = item.itemState & ODS_SELECTED != 0;
        if selected {
            SetDCBrushColor(item.hDC, SELECTED);
            let selection = RECT {
                left: r.left + self.scale(2),
                top: r.top + self.scale(3),
                right: r.right - self.scale(3),
                bottom: r.bottom - self.scale(3),
            };
            FillRect(item.hDC, &selection, GetStockObject(DC_BRUSH) as HBRUSH);
            let marker = RECT {
                left: r.left + self.scale(3),
                top: r.top + self.scale(21),
                right: r.left + self.scale(6),
                bottom: r.bottom - self.scale(21),
            };
            SetDCBrushColor(item.hDC, ACCENT);
            FillRect(item.hDC, &marker, GetStockObject(DC_BRUSH) as HBRUSH);
        }
        let mut title = RECT {
            left: r.left + self.scale(16),
            top: r.top + self.scale(15),
            right: r.right - self.scale(14),
            bottom: r.top + self.scale(39),
        };
        draw_text(
            item.hDC,
            &skill.name,
            self.fonts.strong,
            INK,
            &mut title,
            DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        let preview: String = skill
            .prompt
            .split_whitespace()
            .take(32)
            .collect::<Vec<_>>()
            .join(" ");
        let mut subtitle = RECT {
            left: title.left,
            top: r.top + self.scale(42),
            right: title.right,
            bottom: r.bottom - self.scale(10),
        };
        draw_text(
            item.hDC,
            &preview,
            self.fonts.small,
            MUTED,
            &mut subtitle,
            DT_SINGLELINE | DT_END_ELLIPSIS,
        );
    }
}

unsafe fn paint_box(dc: HDC, rect: &RECT, fill: u32, border: u32) {
    SetDCBrushColor(dc, fill);
    FillRect(dc, rect, GetStockObject(DC_BRUSH) as HBRUSH);
    if border != fill {
        SetDCBrushColor(dc, border);
        FrameRect(dc, rect, GetStockObject(DC_BRUSH) as HBRUSH);
    }
}

unsafe fn draw_text(dc: HDC, text: &str, font: HFONT, color: u32, rect: &mut RECT, flags: u32) {
    let original = SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, color);
    let text = wide(text);
    DrawTextW(
        dc,
        text.as_ptr(),
        (text.len() - 1) as i32,
        rect,
        flags | DT_NOPREFIX,
    );
    SelectObject(dc, original);
}

unsafe fn copy_to_clipboard(hwnd: HWND, text: &str) -> Result<(), ()> {
    let value = wide(&windows_lines(text));
    let memory = GlobalAlloc(GMEM_MOVEABLE, value.len() * size_of::<u16>());
    if memory.is_null() {
        return Err(());
    }
    let destination = GlobalLock(memory) as *mut u16;
    if destination.is_null() {
        GlobalFree(memory);
        return Err(());
    }
    std::ptr::copy_nonoverlapping(value.as_ptr(), destination, value.len());
    GlobalUnlock(memory);
    if OpenClipboard(hwnd) == 0 {
        GlobalFree(memory);
        return Err(());
    }
    let success = EmptyClipboard() != 0 && !SetClipboardData(13, memory).is_null();
    CloseClipboard();
    if success {
        Ok(())
    } else {
        GlobalFree(memory);
        Err(())
    }
}

unsafe fn restore(hwnd: HWND) {
    if IsIconic(hwnd) != 0 {
        ShowWindow(hwnd, SW_RESTORE);
    } else {
        SetWindowPos(
            hwnd,
            null_mut(),
            0,
            0,
            0,
            0,
            SWP_SHOWWINDOW | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        );
    }
    SetForegroundWindow(hwnd);
}

// Borderless EDIT controls do not provide wheel scrolling without WS_VSCROLL.
// Queue wheel input to the parent while keeping native text/selection behavior.
unsafe extern "system" fn prompt_proc(
    hwnd: HWND,
    message: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    owner: usize,
) -> LRESULT {
    if message == WM_MOUSEWHEEL {
        PostMessageW(owner as HWND, message, wp, lp);
        return 0;
    }
    if message == WM_NCDESTROY {
        RemoveWindowSubclass(hwnd, Some(prompt_proc), id);
    }
    DefSubclassProc(hwnd, message, wp, lp)
}

// Synchronous control notifications only enqueue actions. RefCell prevents a nested
// Windows callback from aliasing mutable UI state during a native API call.
unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lp as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<App>;
    match message {
        WM_DESTROY => {
            PostQuitMessage(0);
            return 0;
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }
        WM_CLOSE => {
            PostMessageW(hwnd, ACTION, EXIT as usize, 0);
            return 0;
        }
        WM_COMMAND => {
            PostMessageW(hwnd, ACTION, wp, lp);
            return 0;
        }
        WM_SYSCOMMAND if wp & 0xfff0 == SC_MINIMIZE as usize => {
            if !pointer.is_null() {
                if let Ok(app) = (*pointer).try_borrow() {
                    if app.tray_added {
                        ShowWindow(hwnd, SW_HIDE);
                        return 0;
                    }
                }
            }
        }
        RESTORE => {
            restore(hwnd);
            return 0;
        }
        TRAY_MESSAGE => {
            let event = lp as u32 & 0xffff;
            if event == NIN_SELECT
                || event == (NIN_SELECT | 1)
                || event == WM_LBUTTONUP
                || event == WM_LBUTTONDBLCLK
            {
                restore(hwnd);
            }
            return 0;
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
            let dc = wp as HDC;
            let id = GetDlgCtrlID(lp as HWND) as u16;
            let is_field = matches!(id, SEARCH | NAME | PROMPT);
            SetTextColor(dc, if is_field { INK } else { MUTED });
            let background = if is_field {
                FIELD
            } else if id == 122 {
                SIDEBAR
            } else {
                PAPER
            };
            SetBkColor(dc, background);
            SetDCBrushColor(dc, background);
            return GetStockObject(DC_BRUSH) as isize;
        }
        WM_CTLCOLORLISTBOX => {
            SetDCBrushColor(wp as HDC, SIDEBAR);
            return GetStockObject(DC_BRUSH) as isize;
        }
        WM_ERASEBKGND => {
            return 1;
        }
        WM_MEASUREITEM => {
            let item = &mut *(lp as *mut MEASUREITEMSTRUCT);
            item.itemHeight = MulDiv(76, GetDpiForWindow(hwnd) as i32, 96) as u32;
            return 1;
        }
        WM_GETMINMAXINFO => {
            let dpi = GetDpiForWindow(hwnd).max(96);
            let info = &mut *(lp as *mut MINMAXINFO);
            let mut monitor: MONITORINFO = zeroed();
            monitor.cbSize = size_of::<MONITORINFO>() as u32;
            GetMonitorInfoW(
                MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            );
            info.ptMinTrackSize.x =
                MulDiv(880, dpi as i32, 96).min(monitor.rcWork.right - monitor.rcWork.left);
            info.ptMinTrackSize.y =
                MulDiv(600, dpi as i32, 96).min(monitor.rcWork.bottom - monitor.rcWork.top);
            return 0;
        }
        WM_QUERYENDSESSION => {
            return 1;
        }
        _ => {}
    }
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wp, lp);
    }
    let state = &*pointer;
    match message {
        WM_MOUSEWHEEL => {
            if let Ok(mut app) = state.try_borrow_mut() {
                let mut point = POINT {
                    x: lp as i16 as i32,
                    y: (lp >> 16) as i16 as i32,
                };
                ScreenToClient(hwnd, &mut point);
                let bounds = app.prompt_bounds();
                if point.x >= app.scale(bounds[0])
                    && point.x < app.scale(bounds[2])
                    && point.y >= app.scale(bounds[1])
                    && point.y < app.scale(bounds[3])
                {
                    app.wheel_scroll((wp >> 16) as i16 as i32);
                    return 0;
                }
            }
        }
        WM_LBUTTONDOWN => {
            if let Ok(mut app) = state.try_borrow_mut() {
                let x = lp as i16 as i32;
                let y = (lp >> 16) as i16 as i32;
                if let Some(scroll) = app.prompt_scroll() {
                    if x >= scroll.track.left - app.scale(5)
                        && x < scroll.track.right + app.scale(5)
                        && y >= scroll.track.top
                        && y < scroll.track.bottom
                    {
                        let offset = if y >= scroll.thumb.top && y < scroll.thumb.bottom {
                            y - scroll.thumb.top
                        } else {
                            (scroll.thumb.bottom - scroll.thumb.top) / 2
                        };
                        app.scroll_drag = Some(offset);
                        SetCapture(hwnd);
                        app.drag_scroll(y);
                        return 0;
                    }
                }
            }
        }
        WM_MOUSEMOVE => {
            if let Ok(app) = state.try_borrow() {
                if app.scroll_drag.is_some() {
                    app.drag_scroll((lp >> 16) as i16 as i32);
                    return 0;
                }
            }
        }
        WM_LBUTTONUP | WM_CAPTURECHANGED => {
            if let Ok(mut app) = state.try_borrow_mut() {
                if app.scroll_drag.take().is_some() {
                    if message == WM_LBUTTONUP {
                        ReleaseCapture();
                    }
                    app.invalidate_scroll();
                    return 0;
                }
            }
        }
        WM_SETFOCUS => {
            if let Ok(app) = state.try_borrow() {
                app.restore_focus();
            }
            return 0;
        }
        WM_PAINT => {
            let mut paint = zeroed();
            let dc = BeginPaint(hwnd, &mut paint);
            if let Ok(app) = state.try_borrow() {
                app.paint(dc);
            }
            EndPaint(hwnd, &paint);
            return 0;
        }
        WM_DRAWITEM => {
            if let Ok(app) = state.try_borrow() {
                app.draw_item(&*(lp as *const DRAWITEMSTRUCT));
            }
            return 1;
        }
        WM_SIZE => {
            if let Ok(app) = state.try_borrow() {
                if wp != SIZE_MINIMIZED as usize {
                    app.layout();
                }
            }
            return 0;
        }
        WM_DPICHANGED => {
            if let Ok(mut app) = state.try_borrow_mut() {
                app.dpi = (wp & 0xffff) as u32;
                let fonts = Fonts::new(app.dpi);
                let previous = std::mem::replace(&mut app.fonts, fonts);
                app.apply_fonts();
                drop(previous);
                let rect = &*(lp as *const RECT);
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                app.layout();
            }
            return 0;
        }
        ACTION => {
            if let Ok(mut app) = state.try_borrow_mut() {
                app.command((wp & 0xffff) as u16, ((wp >> 16) & 0xffff) as u16);
            }
            return 0;
        }
        _ => {
            if let Ok(mut app) = state.try_borrow_mut() {
                if message == app.taskbar_created {
                    app.add_tray();
                    return 0;
                }
            }
        }
    }
    DefWindowProcW(hwnd, message, wp, lp)
}

pub unsafe fn run() {
    let launch_minimized = std::env::args_os().any(|argument| argument == "--minimized");
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let mutex = CreateMutexW(
        null(),
        0,
        wide("Local\\CriticalDeveloper.AISkillview.Instance").as_ptr(),
    );
    if mutex.is_null() {
        show_startup_error("Windows could not create the application lock.");
        return;
    }
    if GetLastError() == ERROR_ALREADY_EXISTS {
        let previous = FindWindowW(wide(CLASS).as_ptr(), null());
        if !previous.is_null() && !launch_minimized {
            PostMessageW(previous, RESTORE, 0, 0);
        }
        CloseHandle(mutex);
        return;
    }
    let store = match Store::load() {
        Ok(store) => store,
        Err(error) => {
            show_startup_error(&error.to_string());
            CloseHandle(mutex);
            return;
        }
    };
    let instance = GetModuleHandleW(null());
    let icon = LoadIconW(instance, 101usize as *const u16);
    let class_name = wide(CLASS);
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        hIcon: icon,
        hIconSm: icon,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        lpszClassName: class_name.as_ptr(),
        ..zeroed()
    };
    if RegisterClassExW(&class) == 0 {
        show_startup_error("Windows could not register the application window.");
        CloseHandle(mutex);
        return;
    }
    let app = Box::new(RefCell::new(App {
        hwnd: null_mut(),
        instance,
        icon,
        dpi: 96,
        fonts: Fonts::new(96),
        controls: Controls::default(),
        focus: null_mut(),
        store,
        visible: Vec::new(),
        selected: None,
        mode: Mode::View,
        original: (String::new(), String::new()),
        status: String::new(),
        status_error: false,
        taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
        tray_added: false,
        scroll_drag: None,
        wheel_remainder: 0,
        startup_enabled: false,
    }));
    let dpi = GetDpiForSystem();
    let mut work: RECT = zeroed();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as *mut _, 0);
    let width = MulDiv(1040, dpi as i32, 96).min(work.right - work.left);
    let height = MulDiv(750, dpi as i32, 96).min(work.bottom - work.top);
    let hwnd = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        class_name.as_ptr(),
        wide(TITLE).as_ptr(),
        WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
        work.left + (work.right - work.left - width) / 2,
        work.top + (work.bottom - work.top - height) / 2,
        width,
        height,
        null_mut(),
        null_mut(),
        instance,
        &*app as *const _ as *const _,
    );
    if hwnd.is_null() {
        show_startup_error("Windows could not open the application window.");
        CloseHandle(mutex);
        return;
    }
    {
        let mut state = app.borrow_mut();
        state.hwnd = hwnd;
        if let Err(error) = state.init() {
            show_startup_error(&error);
            DestroyWindow(hwnd);
            CloseHandle(mutex);
            return;
        }
    }
    if !launch_minimized || !app.borrow().tray_added {
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        SetFocus(app.borrow().controls.new);
        RedrawWindow(
            hwnd,
            null(),
            null_mut(),
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
        );
    }
    let mut message: MSG = zeroed();
    loop {
        let result = GetMessageW(&mut message, null_mut(), 0, 0);
        if result <= 0 {
            break;
        }
        if message.message == WM_KEYDOWN {
            let control = GetKeyState(VK_CONTROL as i32) < 0;
            let id = match message.wParam as u16 {
                0x4e if control => Some(NEW),
                0x46 if control => {
                    let search = app.borrow().controls.search;
                    SetFocus(search);
                    SendMessageW(search, EM_SETSEL, 0, -1);
                    continue;
                }
                0x53 if control => Some(SAVE),
                VK_RETURN if control => Some(COPY),
                VK_RETURN => {
                    let focus = GetFocus();
                    let id = GetDlgCtrlID(focus) as u16;
                    if matches!(
                        id,
                        NEW | EDIT | DELETE | CANCEL | SAVE | COPY | FIRST | AUTOSTART
                    ) && IsWindowEnabled(focus) != 0
                    {
                        Some(id)
                    } else {
                        None
                    }
                }
                VK_ESCAPE => {
                    let state = app.borrow();
                    if state.mode != Mode::View {
                        Some(CANCEL)
                    } else {
                        App::set_text(state.controls.search, "");
                        None
                    }
                }
                _ => None,
            };
            if let Some(id) = id {
                PostMessageW(hwnd, ACTION, id as usize, 0);
                continue;
            }
        }
        if IsDialogMessageW(hwnd, &message) == 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
            let state = app.borrow();
            if message.hwnd == state.controls.prompt
                && matches!(
                    message.message,
                    WM_KEYDOWN | WM_KEYUP | WM_CHAR | WM_MOUSEWHEEL | WM_LBUTTONUP | WM_TIMER
                )
            {
                state.invalidate_scroll();
            }
        }
    }
    app.borrow().remove_tray();
    CloseHandle(mutex);
}

unsafe fn show_startup_error(message: &str) {
    MessageBoxW(
        null_mut(),
        wide(message).as_ptr(),
        wide(TITLE).as_ptr(),
        MB_OK | MB_ICONERROR,
    );
}
