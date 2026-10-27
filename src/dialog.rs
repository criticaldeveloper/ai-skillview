use std::cell::Cell;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::WindowsProgramming::MulDiv;
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_SELECTED};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "CriticalDeveloper.AISkillview.Confirmation";
const PAPER: u32 = rgb(252, 251, 248);
const INK: u32 = rgb(36, 43, 41);
const MUTED: u32 = rgb(108, 113, 109);
const LINE: u32 = rgb(221, 223, 216);
const ACCENT: u32 = rgb(173, 65, 33);
const PRESSED: u32 = rgb(139, 49, 25);
const BUTTON_PRESSED: u32 = rgb(234, 230, 219);
const TITLE_ID: usize = 301;
const BODY_ID: usize = 302;
const FIRST_BUTTON: usize = 310;
const STATIC_NO_PREFIX: u32 = 0x0080;

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[derive(Clone, Copy)]
pub enum DialogKind {
    SaveChanges,
    Delete,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogResult {
    Save,
    Discard,
    Confirm,
    Cancel,
}

struct Button {
    label: &'static str,
    result: DialogResult,
    primary: bool,
    hwnd: Cell<HWND>,
}

impl Button {
    fn new(label: &'static str, result: DialogResult, primary: bool) -> Self {
        Self {
            label,
            result,
            primary,
            hwnd: Cell::new(null_mut()),
        }
    }
}

struct Fonts {
    title: HFONT,
    body: HFONT,
    button: HFONT,
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
                DEFAULT_PITCH as u32,
                wide("Segoe UI").as_ptr(),
            )
        }
        Self {
            title: font(dpi, 22, FW_SEMIBOLD),
            body: font(dpi, 15, FW_NORMAL),
            button: font(dpi, 14, FW_SEMIBOLD),
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            for font in [self.title, self.body, self.button] {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
        }
    }
}

struct Dialog {
    dpi: u32,
    fonts: Fonts,
    buttons: Vec<Button>,
    focused_button: Cell<usize>,
    result: Cell<DialogResult>,
    closed: Cell<bool>,
}

impl Dialog {
    fn scale(&self, value: i32) -> i32 {
        unsafe { MulDiv(value, self.dpi as i32, 96) }
    }

    unsafe fn finish(&self, hwnd: HWND, result: DialogResult) {
        self.result.set(result);
        self.closed.set(true);
        DestroyWindow(hwnd);
    }

    unsafe fn paint(&self, hwnd: HWND, dc: HDC) {
        let mut rect: RECT = zeroed();
        GetClientRect(hwnd, &mut rect);
        SetDCBrushColor(dc, PAPER);
        FillRect(dc, &rect, GetStockObject(DC_BRUSH) as HBRUSH);
        SetDCBrushColor(dc, LINE);
        FrameRect(dc, &rect, GetStockObject(DC_BRUSH) as HBRUSH);
        let accent = RECT {
            left: 1,
            top: 1,
            right: rect.right - 1,
            bottom: self.scale(3) + 1,
        };
        SetDCBrushColor(dc, ACCENT);
        FillRect(dc, &accent, GetStockObject(DC_BRUSH) as HBRUSH);
    }

    unsafe fn draw_button(&self, item: &DRAWITEMSTRUCT) {
        let Some(button) = (item.CtlID as usize)
            .checked_sub(FIRST_BUTTON)
            .and_then(|index| self.buttons.get(index))
        else {
            return;
        };
        let pressed = item.itemState & ODS_SELECTED != 0;
        let background = if button.primary {
            if pressed {
                PRESSED
            } else {
                ACCENT
            }
        } else if pressed {
            BUTTON_PRESSED
        } else {
            PAPER
        };
        SetDCBrushColor(item.hDC, background);
        FillRect(item.hDC, &item.rcItem, GetStockObject(DC_BRUSH) as HBRUSH);
        if !button.primary {
            SetDCBrushColor(item.hDC, LINE);
            FrameRect(item.hDC, &item.rcItem, GetStockObject(DC_BRUSH) as HBRUSH);
        }
        let old_font = SelectObject(item.hDC, self.fonts.button);
        SetBkMode(item.hDC, TRANSPARENT as i32);
        SetTextColor(item.hDC, if button.primary { PAPER } else { INK });
        let mut rect = item.rcItem;
        DrawTextW(
            item.hDC,
            wide(button.label).as_ptr(),
            -1,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        SelectObject(item.hDC, old_font);
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lp as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Dialog;
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wp, lp);
    }
    let dialog = &*pointer;
    match message {
        WM_SETFOCUS => {
            if let Some(button) = dialog.buttons.get(dialog.focused_button.get()) {
                let focus = button.hwnd.get();
                if IsWindowVisible(focus) != 0 && IsWindowEnabled(focus) != 0 {
                    SetFocus(focus);
                }
            }
            return 0;
        }
        WM_CLOSE => {
            dialog.finish(hwnd, DialogResult::Cancel);
            return 0;
        }
        WM_DESTROY => {
            dialog.closed.set(true);
            return 0;
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }
        WM_COMMAND if (wp >> 16) as u32 == BN_SETFOCUS => {
            if let Some(index) = (wp & 0xffff)
                .checked_sub(FIRST_BUTTON)
                .filter(|index| *index < dialog.buttons.len())
            {
                dialog.focused_button.set(index);
            }
            return 0;
        }
        WM_COMMAND if (wp >> 16) as u32 == BN_CLICKED => {
            if let Some(button) = (wp & 0xffff)
                .checked_sub(FIRST_BUTTON)
                .and_then(|index| dialog.buttons.get(index))
            {
                dialog.finish(hwnd, button.result);
            }
            return 0;
        }
        WM_DRAWITEM => {
            dialog.draw_button(&*(lp as *const DRAWITEMSTRUCT));
            return 1;
        }
        WM_CTLCOLORSTATIC => {
            let dc = wp as HDC;
            SetBkColor(dc, PAPER);
            SetTextColor(
                dc,
                if GetDlgCtrlID(lp as HWND) == TITLE_ID as i32 {
                    INK
                } else {
                    MUTED
                },
            );
            SetDCBrushColor(dc, PAPER);
            return GetStockObject(DC_BRUSH) as isize;
        }
        WM_ERASEBKGND => {
            return 1;
        }
        WM_PAINT => {
            let mut paint = zeroed();
            let dc = BeginPaint(hwnd, &mut paint);
            dialog.paint(hwnd, dc);
            EndPaint(hwnd, &paint);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, message, wp, lp)
}

unsafe fn register_class(instance: HINSTANCE) -> bool {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    *REGISTERED.get_or_init(|| {
        let class_name = wide(CLASS);
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_DROPSHADOW | CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: class_name.as_ptr(),
            ..zeroed()
        };
        RegisterClassExW(&class) != 0
    })
}

unsafe fn line_height(dc: HDC, font: HFONT) -> i32 {
    let old_font = SelectObject(dc, font);
    let mut metrics: TEXTMETRICW = zeroed();
    GetTextMetricsW(dc, &mut metrics);
    SelectObject(dc, old_font);
    (metrics.tmHeight + metrics.tmExternalLeading).max(1)
}

// Explicit wrapping also keeps long file paths and unbroken names inside the card.
unsafe fn wrap_text(dc: HDC, font: HFONT, text: &str, width: i32) -> Vec<String> {
    let old_font = SelectObject(dc, font);
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut last_break = None;
    for character in text.replace("\r\n", "\n").chars() {
        if character == '\n' {
            lines.push(std::mem::take(&mut line));
            last_break = None;
            continue;
        }
        let previous_length = line.len();
        line.push(character);
        let value = wide(&line);
        let mut size: SIZE = zeroed();
        GetTextExtentPoint32W(dc, value.as_ptr(), (value.len() - 1) as i32, &mut size);
        if size.cx > width && previous_length > 0 {
            let split = last_break
                .filter(|position| *position > 0)
                .unwrap_or(previous_length);
            let remainder = line.split_off(split);
            lines.push(std::mem::replace(&mut line, remainder));
            last_break = line
                .char_indices()
                .filter(|(_, character)| character.is_whitespace())
                .map(|(index, character)| index + character.len_utf8())
                .next_back();
        }
        if character.is_whitespace() {
            last_break = Some(line.len());
        }
    }
    lines.push(line);
    SelectObject(dc, old_font);
    lines
}

/// Shows a matching, frameless native card while keeping the owner's App state independent.
/// Failure, Escape, Alt+F4, and process shutdown all resolve to the safe Cancel result.
pub unsafe fn show(parent: HWND, title: &str, message: &str, kind: DialogKind) -> DialogResult {
    if IsWindow(parent) == 0 {
        return DialogResult::Cancel;
    }
    let instance = GetModuleHandleW(null());
    if !register_class(instance) {
        return DialogResult::Cancel;
    }
    let dpi = GetDpiForWindow(parent).max(96);
    let buttons = match kind {
        DialogKind::SaveChanges => vec![
            Button::new("Cancel", DialogResult::Cancel, false),
            Button::new("Discard", DialogResult::Discard, false),
            Button::new("Save skill", DialogResult::Save, true),
        ],
        DialogKind::Delete => vec![
            Button::new("Cancel", DialogResult::Cancel, false),
            Button::new("Delete skill", DialogResult::Confirm, true),
        ],
        DialogKind::Error => vec![Button::new("OK", DialogResult::Confirm, true)],
    };
    let dialog = Box::new(Dialog {
        dpi,
        fonts: Fonts::new(dpi),
        buttons,
        focused_button: Cell::new(0),
        result: Cell::new(DialogResult::Cancel),
        closed: Cell::new(false),
    });
    let mut owner_rect: RECT = zeroed();
    GetClientRect(parent, &mut owner_rect);
    MapWindowPoints(
        parent,
        null_mut(),
        &mut owner_rect as *mut _ as *mut POINT,
        2,
    );
    let mut monitor: MONITORINFO = zeroed();
    monitor.cbSize = size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(
        MonitorFromWindow(parent, MONITOR_DEFAULTTONEAREST),
        &mut monitor,
    ) == 0
    {
        return DialogResult::Cancel;
    }
    let available_width = (owner_rect.right - owner_rect.left - dialog.scale(32))
        .min(monitor.rcWork.right - monitor.rcWork.left - dialog.scale(32));
    let width = dialog
        .scale(540)
        .min(available_width)
        .max(dialog.scale(280));
    let margin = dialog.scale(28);
    let text_width = width - margin * 2;
    let max_height = (owner_rect.bottom - owner_rect.top - dialog.scale(32))
        .min(monitor.rcWork.bottom - monitor.rcWork.top - dialog.scale(32));
    let dc = GetDC(parent);
    if dc.is_null() {
        return DialogResult::Cancel;
    }
    let title_lines = wrap_text(dc, dialog.fonts.title, title, text_width);
    let mut body_lines = wrap_text(dc, dialog.fonts.body, message, text_width);
    let title_line_height = line_height(dc, dialog.fonts.title);
    let body_line_height = line_height(dc, dialog.fonts.body);
    ReleaseDC(parent, dc);
    let title_height = title_line_height * title_lines.len() as i32 + dialog.scale(3);
    let message_top = margin + title_height + dialog.scale(14);
    let footer_height = dialog.scale(94);
    let max_body_height = (max_height - message_top - footer_height).max(body_line_height);
    let maximum_lines = (max_body_height / body_line_height).max(1) as usize;
    if body_lines.len() > maximum_lines {
        body_lines.truncate(maximum_lines);
        if let Some(last) = body_lines.last_mut() {
            // Reserve visible room for the continuation marker without breaking UTF-8.
            for _ in 0..3 {
                last.pop();
            }
            last.push('\u{2026}');
        }
    }
    let message_height =
        (body_line_height * body_lines.len() as i32 + dialog.scale(4)).max(dialog.scale(38));
    let height = message_top + message_height + footer_height;
    let x = (owner_rect.left + (owner_rect.right - owner_rect.left - width) / 2).clamp(
        monitor.rcWork.left,
        (monitor.rcWork.right - width).max(monitor.rcWork.left),
    );
    let y = (owner_rect.top + (owner_rect.bottom - owner_rect.top - height) / 2).clamp(
        monitor.rcWork.top,
        (monitor.rcWork.bottom - height).max(monitor.rcWork.top),
    );
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_CONTROLPARENT,
        wide(CLASS).as_ptr(),
        wide(title).as_ptr(),
        WS_POPUP | WS_CLIPCHILDREN,
        x,
        y,
        width,
        height,
        parent,
        null_mut(),
        instance,
        &*dialog as *const _ as *const _,
    );
    if hwnd.is_null() {
        return DialogResult::Cancel;
    }
    let title_control = CreateWindowExW(
        0,
        wide("STATIC").as_ptr(),
        wide(&title_lines.join("\r\n")).as_ptr(),
        WS_CHILD | WS_VISIBLE | STATIC_NO_PREFIX,
        margin,
        margin,
        text_width,
        title_height,
        hwnd,
        TITLE_ID as HMENU,
        instance,
        null(),
    );
    let message_control = CreateWindowExW(
        0,
        wide("STATIC").as_ptr(),
        wide(&body_lines.join("\r\n")).as_ptr(),
        WS_CHILD | WS_VISIBLE | STATIC_NO_PREFIX,
        margin,
        message_top,
        text_width,
        message_height,
        hwnd,
        BODY_ID as HMENU,
        instance,
        null(),
    );
    SendMessageW(title_control, WM_SETFONT, dialog.fonts.title as usize, 0);
    SendMessageW(message_control, WM_SETFONT, dialog.fonts.body as usize, 0);
    let gap = dialog.scale(10);
    let button_width = dialog
        .scale(112)
        .min((text_width - gap * (dialog.buttons.len() as i32 - 1)) / dialog.buttons.len() as i32);
    let group_width =
        button_width * dialog.buttons.len() as i32 + gap * (dialog.buttons.len() as i32 - 1);
    for (index, button) in dialog.buttons.iter().enumerate() {
        let control = CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide(button.label).as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32 | BS_NOTIFY as u32,
            width - margin - group_width + index as i32 * (button_width + gap),
            height - margin - dialog.scale(40),
            button_width,
            dialog.scale(40),
            hwnd,
            (FIRST_BUTTON + index) as HMENU,
            instance,
            null(),
        );
        button.hwnd.set(control);
        SendMessageW(control, WM_SETFONT, dialog.fonts.button as usize, 0);
    }
    if title_control.is_null()
        || message_control.is_null()
        || dialog
            .buttons
            .iter()
            .any(|button| button.hwnd.get().is_null())
    {
        DestroyWindow(hwnd);
        return DialogResult::Cancel;
    }
    let previous_focus = GetFocus();
    let owner_was_enabled = IsWindowEnabled(parent) != 0;
    if owner_was_enabled {
        EnableWindow(parent, 0);
    }
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    SetFocus(dialog.buttons[0].hwnd.get());
    RedrawWindow(
        hwnd,
        null(),
        null_mut(),
        RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
    );
    let mut quit_code = None;
    let mut message: MSG = zeroed();
    while !dialog.closed.get() {
        let received = GetMessageW(&mut message, null_mut(), 0, 0);
        if received <= 0 {
            if received == 0 {
                quit_code = Some(message.wParam as i32);
            }
            break;
        }
        let belongs_to_dialog = message.hwnd == hwnd || IsChild(hwnd, message.hwnd) != 0;
        if belongs_to_dialog && message.message == WM_KEYDOWN {
            match message.wParam as u16 {
                VK_ESCAPE => {
                    dialog.finish(hwnd, DialogResult::Cancel);
                    continue;
                }
                VK_RETURN => {
                    let focus = GetFocus();
                    if let Some(button) = dialog
                        .buttons
                        .iter()
                        .find(|button| button.hwnd.get() == focus)
                    {
                        dialog.finish(hwnd, button.result);
                    }
                    continue;
                }
                VK_TAB => {
                    let focus = GetFocus();
                    let current = dialog
                        .buttons
                        .iter()
                        .position(|button| button.hwnd.get() == focus)
                        .unwrap_or(0);
                    let count = dialog.buttons.len();
                    let next = if GetKeyState(VK_SHIFT as i32) < 0 {
                        (current + count - 1) % count
                    } else {
                        (current + 1) % count
                    };
                    SetFocus(dialog.buttons[next].hwnd.get());
                    continue;
                }
                _ => {}
            }
        }
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    if IsWindow(hwnd) != 0 {
        DestroyWindow(hwnd);
    }
    if owner_was_enabled && IsWindow(parent) != 0 {
        EnableWindow(parent, 1);
        SetForegroundWindow(parent);
        if IsWindow(previous_focus) != 0 && IsWindowEnabled(previous_focus) != 0 {
            SetFocus(previous_focus);
        }
    }
    if let Some(code) = quit_code {
        PostQuitMessage(code);
    }
    dialog.result.get()
}
