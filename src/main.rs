//! Dyno - memory cleaner and system optimizer (Rust, plain Win32).
#![windows_subsystem = "windows"]

mod mem;
mod scan;
mod settings;
mod tweaks;
mod update;

use core::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows_sys::core::PCWSTR;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// ------------------------------------------------------------------ constants
const CLASS: &str = "DynoMainWindow";
const WIN_W: i32 = 540;
const WIN_H: i32 = 760;
const STYLE: u32 = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
const WM_APP_TRAY: u32 = WM_APP + 1;
const WM_APP_REFRESH: u32 = WM_APP + 2;

const ID_TIMER: usize = 1;
const CMD_SHOW: usize = 1001;
const CMD_CLEAN: usize = 1002;
const CMD_EXIT: usize = 1003;
const CMD_APPLY_SAFE: usize = 1004;
const CMD_CHECK_UPDATE: usize = 1005;
const COOLDOWN: Duration = Duration::from_secs(120);

const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

const BG: u32 = rgb(0x0f, 0x10, 0x12);
const CARD: u32 = rgb(0x17, 0x18, 0x1b);
const LINE: u32 = rgb(0x26, 0x28, 0x2c);
const OFF: u32 = rgb(0x3a, 0x3d, 0x43);
const TXT: u32 = rgb(0xe8, 0xe8, 0xea);
const MUT: u32 = rgb(0x9a, 0x9c, 0xa3);
const RED: u32 = rgb(0xe0, 0x24, 0x24);
const RED2: u32 = rgb(0xff, 0x6b, 0x5b);
const WHITE: u32 = rgb(0xff, 0xff, 0xff);
const GREEN: u32 = rgb(0x3e, 0xcf, 0x8e);
const GREEN_BG: u32 = rgb(0x10, 0x30, 0x1f);
const AMBER: u32 = rgb(0xf0, 0xb4, 0x29);
const AMBER_BG: u32 = rgb(0x3a, 0x2d, 0x0f);
const RED_DIM: u32 = rgb(0x4a, 0x14, 0x14);

// font slots
const F_BODY: usize = 0; // 13 px
const F_BOLD: usize = 1; // 14 px bold
const F_SMALL: usize = 2; // 12 px
const F_HEAD: usize = 3; // 16 px bold
const F_NUM: usize = 4; // 22 px bold
const F_BIG: usize = 5; // 28 px bold
const F_HUGE: usize = 6; // 44 px bold
const F_PILL: usize = 7; // 11 px bold

#[derive(Clone, Copy)]
struct R {
    l: i32,
    t: i32,
    r: i32,
    b: i32,
}

impl R {
    const fn new(l: i32, t: i32, r: i32, b: i32) -> R {
        R { l, t, r, b }
    }
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.l && x < self.r && y >= self.t && y < self.b
    }
}

// Buttons (shared by drawing and click handling).
const CLEAN_BTN: R = R::new(36, 100, 176, 136);
const RECO_BTN: R = R::new(304, 696, 424, 732);
const APPLY_BTN: R = R::new(436, 696, 520, 732);
const CANCEL_BTN: R = R::new(432, 696, 520, 732);
const DONE_BTN: R = R::new(432, 696, 520, 732);
const RESTART_NOW_BTN: R = R::new(386, 696, 520, 732);
const RESTART_LATER_BTN: R = R::new(262, 696, 374, 732);
const CONFIRM_YES_BTN: R = R::new(280, 400, 430, 440);
const CONFIRM_NO_BTN: R = R::new(110, 400, 260, 440);
const UPDATE_RETRY_BTN: R = R::new(280, 580, 430, 620);
const UPDATE_CANCEL_BTN: R = R::new(110, 580, 260, 620);
const TAB_Y: i32 = 164;
const TAB_H: i32 = 30;
const LIST_Y: i32 = 206;
const ROW_STEP: i32 = 54;
const ROW_H: i32 = 48;
const FOOTER_Y: i32 = 684;

#[derive(Clone, Copy, PartialEq)]
enum UiMode {
    Dashboard,
    Confirm,
    Applying,
    Results,
    Updating,
}

struct StepResult {
    name: &'static str,
    ok: bool,
    detail: String,
    reboot: bool,
}

// ---------------------------------------------------------------------- state
struct AppState {
    st: settings::Settings,
    facts: Option<scan::Facts>,
    mode: UiMode,
    active_tab: usize,
    scroll_y: i32,
    used: u64,
    total: u64,
    busy_clean: bool,
    last_clean: Option<Instant>,
    last_freed: u64,
    ticks: u32,

    // applying
    apply_steps: Vec<String>,
    apply_flags: Vec<u8>, // 0 waiting, 1 done, 2 skipped
    apply_cur: usize,

    // results
    results: Vec<StepResult>,
    res_title: String,
    res_sub: String,
    freed_ram: u64,
    freed_temp: u64,
    reboot_count: usize,

    // updating
    update_ver: String,
    update_status: String,
    update_detail: String,
    update_pct: f32,
    update_step: u8,
    update_err: Option<String>,

    fonts: [usize; 8],
    hicon: usize,
    icon: usize,
}

impl AppState {
    const fn initial() -> AppState {
        AppState {
            st: settings::Settings { areas: [true; 4], auto: true, thr: 85, tweaks: [false; tweaks::COUNT] },
            facts: None,
            mode: UiMode::Dashboard,
            active_tab: 0,
            scroll_y: 0,
            used: 0,
            total: 1,
            busy_clean: false,
            last_clean: None,
            last_freed: 0,
            ticks: 0,
            apply_steps: Vec::new(),
            apply_flags: Vec::new(),
            apply_cur: 0,
            results: Vec::new(),
            res_title: String::new(),
            res_sub: String::new(),
            freed_ram: 0,
            freed_temp: 0,
            reboot_count: 0,
            update_ver: String::new(),
            update_status: String::new(),
            update_detail: String::new(),
            update_pct: 0.0,
            update_step: 0,
            update_err: None,
            fonts: [0; 8],
            hicon: 0,
            icon: 0,
        }
    }
}

static APP: Mutex<AppState> = Mutex::new(AppState::initial());
static HWND_MAIN: AtomicUsize = AtomicUsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
static CANCEL: AtomicBool = AtomicBool::new(false);

// -------------------------------------------------------------------- helpers
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn human(b: u64) -> String {
    let gb = b as f64 / 1_073_741_824.0;
    if gb >= 1.0 {
        format!("{:.1} GB", gb)
    } else {
        format!("{} MB", (b as f64 / 1_048_576.0).round() as u64)
    }
}

fn percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 * 100.0 / total as f64
    }
}

fn ago(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        "just now".to_string()
    } else if s < 3600 {
        format!("{} min ago", s / 60)
    } else {
        format!("{} h ago", s / 3600)
    }
}

fn sc(v: i32, s: f32) -> i32 {
    (v as f32 * s).round() as i32
}

unsafe fn scale(hwnd: HWND) -> f32 {
    let d = GetDpiForWindow(hwnd);
    if d == 0 {
        1.0
    } else {
        d as f32 / 96.0
    }
}

// -------------------------------------------------------------------- drawing
struct Ctx {
    hdc: HDC,
    s: f32,
    fonts: [usize; 8],
}

impl Ctx {
    unsafe fn fill(&self, l: i32, t: i32, r: i32, b: i32, color: u32) {
        let br = CreateSolidBrush(color);
        let rc = RECT { left: sc(l, self.s), top: sc(t, self.s), right: sc(r, self.s), bottom: sc(b, self.s) };
        FillRect(self.hdc, &rc, br);
        DeleteObject(br as HGDIOBJ);
    }

    unsafe fn rrect(&self, rc: R, rad: i32, fill: u32, border: Option<u32>) {
        let br = CreateSolidBrush(fill);
        let pen = match border {
            Some(c) => CreatePen(0, 1, c),
            None => CreatePen(5, 0, 0),
        };
        let ob = SelectObject(self.hdc, br as HGDIOBJ);
        let op = SelectObject(self.hdc, pen as HGDIOBJ);
        let d = sc(rad * 2, self.s);
        RoundRect(self.hdc, sc(rc.l, self.s), sc(rc.t, self.s), sc(rc.r, self.s) + 1, sc(rc.b, self.s) + 1, d, d);
        SelectObject(self.hdc, ob);
        SelectObject(self.hdc, op);
        DeleteObject(br as HGDIOBJ);
        DeleteObject(pen as HGDIOBJ);
    }

    unsafe fn circle(&self, rc: R, fill: u32, border: Option<u32>) {
        let br = CreateSolidBrush(fill);
        let pen = match border {
            Some(c) => CreatePen(0, 1, c),
            None => CreatePen(5, 0, 0),
        };
        let ob = SelectObject(self.hdc, br as HGDIOBJ);
        let op = SelectObject(self.hdc, pen as HGDIOBJ);
        Ellipse(self.hdc, sc(rc.l, self.s), sc(rc.t, self.s), sc(rc.r, self.s) + 1, sc(rc.b, self.s) + 1);
        SelectObject(self.hdc, ob);
        SelectObject(self.hdc, op);
        DeleteObject(br as HGDIOBJ);
        DeleteObject(pen as HGDIOBJ);
    }

    unsafe fn check(&self, cx: i32, cy: i32, color: u32) {
        let pen = CreatePen(0, sc(2, self.s).max(1), color);
        let op = SelectObject(self.hdc, pen as HGDIOBJ);
        MoveToEx(self.hdc, sc(cx - 5, self.s), sc(cy, self.s), null_mut());
        LineTo(self.hdc, sc(cx - 2, self.s), sc(cy + 4, self.s));
        LineTo(self.hdc, sc(cx + 5, self.s), sc(cy - 4, self.s));
        SelectObject(self.hdc, op);
        DeleteObject(pen as HGDIOBJ);
    }

    unsafe fn text(&self, rc: R, s: &str, color: u32, font: usize, flags: u32) {
        if s.is_empty() {
            return;
        }
        let old = SelectObject(self.hdc, self.fonts[font] as HGDIOBJ);
        SetTextColor(self.hdc, color);
        let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
        let mut r = RECT { left: sc(rc.l, self.s), top: sc(rc.t, self.s), right: sc(rc.r, self.s), bottom: sc(rc.b, self.s) };
        DrawTextW(self.hdc, w.as_ptr(), -1, &mut r, flags | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
        SelectObject(self.hdc, old);
    }

    unsafe fn button(&self, rc: R, label: &str, primary: bool, dim: bool) {
        if primary {
            self.rrect(rc, 6, if dim { RED_DIM } else { RED }, None);
            self.text(rc, label, WHITE, F_BOLD, DT_CENTER);
        } else {
            self.rrect(rc, 6, CARD, Some(OFF));
            self.text(rc, label, TXT, F_BODY, DT_CENTER);
        }
    }

    unsafe fn toggle(&self, l: i32, t: i32, on: bool) {
        self.rrect(R::new(l, t, l + 38, t + 22), 11, if on { RED } else { OFF }, None);
        let kx = if on { l + 19 } else { l + 3 };
        self.circle(R::new(kx, t + 3, kx + 16, t + 19), WHITE, None);
    }

    unsafe fn pill(&self, r: R, risk: u8) {
        let (bg, fg, label) = match risk {
            tweaks::HIGH => (RED_DIM, RED2, "High Risk"),
            tweaks::MID => (AMBER_BG, AMBER, "Moderate"),
            _ => (GREEN_BG, GREEN, "Safe"),
        };
        self.rrect(r, 10, bg, None);
        self.text(r, label, fg, F_PILL, DT_CENTER);
    }

    /// status: 0 waiting, 1 working, 2 done, 3 skipped
    unsafe fn status_icon(&self, cx: i32, cy: i32, status: u8) {
        let r = R::new(cx - 9, cy - 9, cx + 9, cy + 9);
        match status {
            2 => {
                self.circle(r, GREEN_BG, None);
                self.check(cx, cy, GREEN);
            }
            3 => {
                self.circle(r, AMBER_BG, None);
                self.text(r, "!", AMBER, F_BOLD, DT_CENTER);
            }
            1 => {
                self.circle(r, RED, None);
                self.circle(R::new(cx - 3, cy - 3, cx + 3, cy + 3), WHITE, None);
            }
            _ => self.circle(r, CARD, Some(OFF)),
        }
    }
}

unsafe fn draw_header(c: &Ctx, _hicon: usize) {
    c.fill(0, 0, WIN_W, WIN_H, BG);
}

unsafe fn draw_dashboard(c: &Ctx, s: &AppState) {
    draw_header(c, s.hicon);

    // RAM card
    let pct = percent(s.used, s.total);
    c.rrect(R::new(20, 16, 520, 152), 8, CARD, Some(LINE));
    c.text(R::new(36, 28, 250, 66), &human(s.used), TXT, F_BIG, DT_LEFT);
    c.text(
        R::new(220, 34, 504, 62),
        &format!("of {} in use \u{b7} {:.0}%", human(s.total), pct),
        MUT,
        F_BODY,
        DT_RIGHT,
    );
    c.rrect(R::new(36, 76, 504, 84), 4, LINE, None);
    let fw = ((468.0 * pct / 100.0).round() as i32).clamp(0, 468);
    if fw >= 8 {
        c.rrect(R::new(36, 76, 36 + fw, 84), 4, RED, None);
    } else if fw > 0 {
        c.fill(36, 76, 36 + fw, 85, RED);
    }
    c.button(CLEAN_BTN, if s.busy_clean { "Cleaning\u{2026}" } else { "Clean memory now" }, true, false);
    let last = match s.last_clean {
        Some(t) => format!("Freed {} {}.", human(s.last_freed), ago(t.elapsed())),
        None => String::new(),
    };
    c.text(R::new(192, 100, 504, 136), &last, MUT, F_SMALL, DT_LEFT);

    // Tabs
    let tab_w = 83;
    for (i, name) in tweaks::CAT_NAMES.iter().enumerate() {
        let l = 21 + i as i32 * tab_w;
        let active = i == s.active_tab;
        c.text(
            R::new(l, TAB_Y, l + tab_w, TAB_Y + TAB_H),
            name,
            if active { RED2 } else { MUT },
            if active { F_BOLD } else { F_BODY },
            DT_CENTER,
        );
        if active {
            c.fill(l + 4, TAB_Y + TAB_H - 2, l + tab_w - 4, TAB_Y + TAB_H, RED);
        }
    }
    c.fill(20, TAB_Y + TAB_H, 520, TAB_Y + TAB_H + 1, LINE);

    // Rows
    let start = tweaks::CAT_START[s.active_tab];
    let end = tweaks::CAT_END[s.active_tab];
    for (n, i) in (start..end).enumerate() {
        let y = LIST_Y + n as i32 * ROW_STEP - s.scroll_y;
        if y + ROW_H < LIST_Y || y > FOOTER_Y - 4 {
            continue;
        }
        let def = &tweaks::TWEAKS[i];
        let pw = match def.risk {
            tweaks::HIGH => 74,
            tweaks::MID => 66,
            _ => 50,
        };
        let pl = 520 - 12 - pw;
        c.rrect(R::new(20, y, 520, y + ROW_H), 8, CARD, Some(LINE));
        c.toggle(32, y + 13, s.st.tweaks[i]);
        c.text(R::new(82, y + 5, pl - 8, y + 25), def.name, TXT, F_BOLD, DT_LEFT | DT_END_ELLIPSIS);
        c.text(R::new(82, y + 25, pl - 8, y + 43), &tweaks::now_text(i, s.facts.as_ref()), MUT, F_SMALL, DT_LEFT | DT_END_ELLIPSIS);
        c.pill(R::new(pl, y + 14, 520 - 12, y + 34), def.risk);
    }

    // Footer
    let sel = s.st.tweaks.iter().filter(|t| **t).count();
    c.fill(0, FOOTER_Y, WIN_W, WIN_H, CARD);
    c.fill(0, FOOTER_Y, WIN_W, FOOTER_Y + 1, LINE);
    c.text(
        R::new(20, FOOTER_Y + 16, 290, FOOTER_Y + 40),
        &format!("{} {} selected", sel, if sel == 1 { "optimization" } else { "optimizations" }),
        TXT,
        F_BOLD,
        DT_LEFT,
    );
    c.button(RECO_BTN, "Recommended", false, false);
    c.button(APPLY_BTN, "Apply", true, sel == 0);
}

unsafe fn draw_confirm(c: &Ctx, s: &AppState) {
    draw_header(c, s.hicon);

    let sel = s.st.tweaks.iter().filter(|t| **t).count();
    let mc = R::new(35, 200, 505, 470);
    c.rrect(mc, 12, CARD, Some(LINE));

    c.circle(R::new(245, 225, 295, 275), RED_DIM, Some(RED));
    c.text(R::new(245, 225, 295, 275), "?", WHITE, F_BIG, DT_CENTER);

    c.text(
        R::new(50, 292, 490, 320),
        "Are you sure you want to apply these optimizations?",
        TXT,
        F_HEAD,
        DT_CENTER,
    );

    c.text(
        R::new(50, 326, 490, 350),
        &format!("{} {} will be applied to your system.", sel, if sel == 1 { "optimization" } else { "optimizations" }),
        MUT,
        F_BODY,
        DT_CENTER,
    );

    c.text(
        R::new(50, 354, 490, 374),
        "Click Optimize to proceed or Cancel to return.",
        MUT,
        F_SMALL,
        DT_CENTER,
    );

    c.button(CONFIRM_NO_BTN, "Cancel", false, false);
    c.button(CONFIRM_YES_BTN, "Optimize", true, false);
}

unsafe fn draw_applying(c: &Ctx, s: &AppState) {
    draw_header(c, s.hicon);
    let total = s.apply_steps.len().max(1);
    let cur = s.apply_cur.min(total);
    let pct = (cur as f32 / total as f32 * 100.0).round();

    c.rrect(R::new(20, 16, 520, 136), 8, CARD, Some(LINE));
    c.text(R::new(36, 28, 504, 82), &format!("{:.0}%", pct), TXT, F_HUGE, DT_CENTER);
    let name = if cur < s.apply_steps.len() { format!("Optimizing: {}\u{2026}", s.apply_steps[cur]) } else { "Finishing optimizations\u{2026}".to_string() };
    c.text(R::new(36, 84, 504, 106), &name, MUT, F_BODY, DT_CENTER);
    c.rrect(R::new(40, 116, 500, 124), 4, LINE, None);
    let fw = ((460.0 * pct / 100.0).round() as i32).clamp(0, 460);
    if fw >= 8 {
        c.rrect(R::new(40, 116, 40 + fw, 124), 4, RED, None);
    }

    let visible = 8usize;
    let n = s.apply_steps.len();
    let first = if n <= visible { 0 } else { cur.saturating_sub(3).min(n - visible) };
    let rows = n.min(visible) as i32;
    c.rrect(R::new(20, 148, 520, 148 + rows * 34 + 12), 8, CARD, Some(LINE));
    for k in 0..(n.min(visible)) {
        let i = first + k;
        let y = 154 + k as i32 * 34;
        let status = if i < s.apply_flags.len() && s.apply_flags[i] == 1 {
            2
        } else if i < s.apply_flags.len() && s.apply_flags[i] == 2 {
            3
        } else if i == cur {
            1
        } else {
            0
        };
        c.status_icon(40, y + 17, status);
        c.text(
            R::new(64, y, 508, y + 34),
            &s.apply_steps[i],
            if status == 0 { MUT } else { TXT },
            F_BODY,
            DT_LEFT | DT_END_ELLIPSIS,
        );
    }
    c.text(R::new(20, 454, 520, 476), "Optimizing your PC in the background...", MUT, F_SMALL, DT_LEFT);

    c.fill(0, FOOTER_Y, WIN_W, WIN_H, CARD);
    c.fill(0, FOOTER_Y, WIN_W, FOOTER_Y + 1, LINE);
    c.text(R::new(20, FOOTER_Y + 12, 400, FOOTER_Y + 48), "Finishing the current change before stopping.", MUT, F_SMALL, DT_LEFT);
    c.button(CANCEL_BTN, "Cancel", false, false);
}

unsafe fn draw_results(c: &Ctx, s: &AppState) {
    draw_header(c, s.hicon);

    c.rrect(R::new(20, 16, 520, 80), 8, CARD, Some(LINE));
    c.circle(R::new(36, 26, 80, 70), GREEN_BG, None);
    c.check(58, 48, GREEN);
    c.text(R::new(94, 28, 508, 50), &s.res_title, TXT, F_HEAD, DT_LEFT | DT_END_ELLIPSIS);
    c.text(R::new(94, 50, 508, 70), &s.res_sub, MUT, F_SMALL, DT_LEFT | DT_END_ELLIPSIS);

    let xs = [20, 182, 345];
    let vals = [human(s.freed_ram), human(s.freed_temp), human(s.used)];
    let labels = ["RAM freed", "Temp files cleared", "RAM in use now"];
    for k in 0..3 {
        let x = xs[k];
        c.rrect(R::new(x, 92, x + 154, 154), 8, CARD, Some(LINE));
        c.text(R::new(x + 12, 98, x + 146, 126), &vals[k], TXT, F_NUM, DT_LEFT);
        c.text(R::new(x + 12, 126, x + 146, 146), labels[k], MUT, F_SMALL, DT_LEFT);
    }

    // Skipped items first, then the ones that worked.
    let mut order: Vec<usize> = (0..s.results.len()).collect();
    order.sort_by_key(|&i| s.results[i].ok);
    let max_rows = 5usize;
    let shown = if order.len() > max_rows { max_rows - 1 } else { order.len() };
    let rows = (if order.len() > max_rows { max_rows } else { order.len() }) as i32;
    c.rrect(R::new(20, 166, 520, 166 + rows * 44 + 12), 8, CARD, Some(LINE));
    for k in 0..shown {
        let r = &s.results[order[k]];
        let y = 172 + k as i32 * 44;
        c.status_icon(40, y + 22, if r.ok { 2 } else { 3 });
        c.text(R::new(64, y + 3, 488, y + 23), r.name, TXT, F_BODY, DT_LEFT | DT_END_ELLIPSIS);
        c.text(R::new(64, y + 23, 488, y + 41), &r.detail, MUT, F_SMALL, DT_LEFT | DT_END_ELLIPSIS);
    }
    if order.len() > max_rows {
        let y = 172 + shown as i32 * 44;
        c.status_icon(40, y + 22, 2);
        c.text(R::new(64, y, 488, y + 44), &format!("{} more changes applied", order.len() - shown), TXT, F_BODY, DT_LEFT);
    }

    if s.reboot_count > 0 {
        c.rrect(R::new(20, 428, 520, 464), 8, AMBER_BG, None);
        c.text(
            R::new(34, 428, 508, 464),
            &format!("Restart needed for {} {} to take effect.", s.reboot_count, if s.reboot_count == 1 { "change" } else { "changes" }),
            AMBER,
            F_SMALL,
            DT_LEFT,
        );
    }

    c.fill(0, FOOTER_Y, WIN_W, WIN_H, CARD);
    c.fill(0, FOOTER_Y, WIN_W, FOOTER_Y + 1, LINE);
    if s.reboot_count > 0 {
        c.text(R::new(20, FOOTER_Y + 12, 240, FOOTER_Y + 48), "Save your work before restarting.", MUT, F_SMALL, DT_LEFT);
        c.button(RESTART_LATER_BTN, "Restart later", false, false);
        c.button(RESTART_NOW_BTN, "Restart now", true, false);
    } else {
        c.button(DONE_BTN, "Done", true, false);
    }
}

unsafe fn draw_updating(c: &Ctx, s: &AppState) {
    draw_header(c, s.hicon);

    let mc = R::new(25, 40, 515, 680);
    c.rrect(mc, 12, CARD, Some(LINE));

    // Circular badge with status symbol
    let icon_rect = R::new(235, 75, 305, 145);
    if s.update_step == 4 {
        // Error
        c.circle(icon_rect, RED_DIM, Some(RED));
        c.text(icon_rect, "!", RED2, F_BIG, DT_CENTER);
    } else if s.update_step == 3 {
        // Done / Restarting
        c.circle(icon_rect, GREEN_BG, Some(GREEN));
        c.check(270, 110, GREEN);
    } else {
        // In progress / downloading
        c.circle(icon_rect, RED_DIM, Some(RED));
        c.text(icon_rect, "\u{2193}", WHITE, F_BIG, DT_CENTER);
    }

    // Title
    let title = if s.update_ver.is_empty() {
        "Updating DynoCore".to_string()
    } else {
        format!("Updating DynoCore to v{}", s.update_ver)
    };
    c.text(R::new(40, 160, 500, 195), &title, TXT, F_HEAD, DT_CENTER);

    // Subtitle / Status
    c.text(R::new(40, 200, 500, 230), &s.update_status, MUT, F_BODY, DT_CENTER);

    // Progress bar
    let bar_left = 55;
    let bar_right = 485;
    let bar_w = bar_right - bar_left;
    let bar_y = 250;
    let bar_h = 12;
    c.rrect(R::new(bar_left, bar_y, bar_right, bar_y + bar_h), 6, LINE, None);
    let fw = ((bar_w as f32 * s.update_pct / 100.0).round() as i32).clamp(0, bar_w);
    if fw >= 6 {
        c.rrect(R::new(bar_left, bar_y, bar_left + fw, bar_y + bar_h), 6, RED, None);
    } else if fw > 0 {
        c.fill(bar_left, bar_y, bar_left + fw, bar_y + bar_h, RED);
    }

    // Percentage & detail
    c.text(
        R::new(400, 272, 485, 296),
        &format!("{:.0}%", s.update_pct),
        TXT,
        F_BOLD,
        DT_RIGHT,
    );
    c.text(
        R::new(55, 272, 395, 296),
        &s.update_detail,
        MUT,
        F_SMALL,
        DT_LEFT | DT_END_ELLIPSIS,
    );

    // Steps checklist card
    let steps_box = R::new(45, 320, 495, 475);
    c.rrect(steps_box, 8, BG, Some(LINE));

    let step_labels = [
        "Check latest release on GitHub",
        "Download latest DynoCore binary",
        "Install update and restart Dyno",
    ];

    for (i, label) in step_labels.iter().enumerate() {
        let sy = 340 + i as i32 * 45;
        let icon_status = if (s.update_step as usize) > i + 1 {
            2 // Done
        } else if (s.update_step as usize) == i + 1 {
            1 // Working
        } else if s.update_step == 4 && (s.update_step as usize) == i + 1 {
            3 // Error
        } else {
            0 // Waiting
        };
        c.status_icon(75, sy + 15, icon_status);
        c.text(
            R::new(100, sy, 475, sy + 30),
            label,
            if icon_status == 0 { MUT } else { TXT },
            F_BODY,
            DT_LEFT | DT_END_ELLIPSIS,
        );
    }

    // Error description or bottom message
    if let Some(ref err) = s.update_err {
        c.rrect(R::new(45, 500, 495, 550), 8, RED_DIM, Some(RED));
        c.text(
            R::new(55, 508, 485, 542),
            err,
            RED2,
            F_SMALL,
            DT_CENTER | DT_END_ELLIPSIS,
        );
        c.button(UPDATE_CANCEL_BTN, "Dismiss", false, false);
        c.button(UPDATE_RETRY_BTN, "Retry", true, false);
    } else {
        c.text(
            R::new(40, 540, 500, 570),
            "Dyno will restart automatically once the update is installed.",
            MUT,
            F_SMALL,
            DT_CENTER,
        );
    }
}

unsafe fn draw(hdc: HDC, hwnd: HWND) {
    let s = scale(hwnd);
    let app = APP.lock().unwrap();
    let c = Ctx { hdc, s, fonts: app.fonts };
    SetBkMode(hdc, 1);
    match app.mode {
        UiMode::Dashboard => draw_dashboard(&c, &app),
        UiMode::Confirm => draw_confirm(&c, &app),
        UiMode::Applying => draw_applying(&c, &app),
        UiMode::Results => draw_results(&c, &app),
        UiMode::Updating => draw_updating(&c, &app),
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc: RECT = zeroed();
    GetClientRect(hwnd, &mut rc);
    let (cw, ch) = (rc.right - rc.left, rc.bottom - rc.top);
    let memdc = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, cw, ch);
    let old = SelectObject(memdc, bmp as HGDIOBJ);
    draw(memdc, hwnd);
    BitBlt(hdc, 0, 0, cw, ch, memdc, 0, 0, SRCCOPY);
    SelectObject(memdc, old);
    DeleteObject(bmp as HGDIOBJ);
    DeleteDC(memdc);
    EndPaint(hwnd, &ps);
}

// ------------------------------------------------------- fonts, icon, window
unsafe fn make_font(px: i32, bold: bool, s: f32) -> usize {
    let face = wide("Segoe UI");
    CreateFontW(-sc(px, s), 0, 0, 0, if bold { 700 } else { 400 }, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr()) as usize
}

unsafe fn make_fonts(hwnd: HWND) {
    let s = scale(hwnd);
    let new = [
        make_font(13, false, s),
        make_font(14, true, s),
        make_font(12, false, s),
        make_font(16, true, s),
        make_font(22, true, s),
        make_font(28, true, s),
        make_font(44, true, s),
        make_font(11, true, s),
    ];
    let hinst = GetModuleHandleW(null());
    let mut hicon = LoadImageW(hinst, 1usize as PCWSTR, IMAGE_ICON, sc(32, s), sc(32, s), LR_DEFAULTCOLOR) as usize;
    if hicon == 0 {
        hicon = LoadIconW(0 as _, IDI_APPLICATION) as usize;
    }
    let mut a = APP.lock().unwrap();
    let olds = a.fonts;
    for old in olds {
        if old != 0 {
            DeleteObject(old as HGDIOBJ);
        }
    }
    a.fonts = new;
    if a.hicon != 0 {
        DestroyIcon(a.hicon as HICON);
    }
    a.hicon = hicon;
}

unsafe fn style_titlebar(hwnd: HWND) {
    let dark: i32 = 1;
    DwmSetWindowAttribute(hwnd, 20, &dark as *const i32 as *const c_void, 4);
    for (attr, color) in [(35u32, 0x0012_100Fu32), (36, 0x00EA_E8E8), (34, 0x0024_24E0)] {
        DwmSetWindowAttribute(hwnd, attr, &color as *const u32 as *const c_void, 4);
    }
}

unsafe fn fit_window(hwnd: HWND) {
    let s = scale(hwnd);
    let dpi = GetDpiForWindow(hwnd).max(96);
    let mut rc = RECT { left: 0, top: 0, right: sc(WIN_W, s), bottom: sc(WIN_H, s) };
    AdjustWindowRectExForDpi(&mut rc, STYLE, 0, 0, dpi);
    let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
    let x = ((GetSystemMetrics(SM_CXSCREEN) - w) / 2).max(0);
    let y = ((GetSystemMetrics(SM_CYSCREEN) - h) / 2).max(0);
    SetWindowPos(hwnd, 0 as _, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
}

// ----------------------------------------------------------------------- tray
unsafe fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut d: NOTIFYICONDATAW = zeroed();
    d.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    d.hWnd = hwnd;
    d.uID = 1;
    d
}

fn set_tip(d: &mut NOTIFYICONDATAW, text: &str) {
    for (i, ch) in text.encode_utf16().take(127).enumerate() {
        d.szTip[i] = ch;
    }
}

fn tip_text() -> String {
    let a = APP.lock().unwrap();
    format!("Dyno: RAM {:.0}% ({} / {})", percent(a.used, a.total), human(a.used), human(a.total))
}

unsafe fn tray_add(hwnd: HWND) {
    let icon = APP.lock().unwrap().icon;
    let mut d = tray_data(hwnd);
    d.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    d.uCallbackMessage = WM_APP_TRAY;
    d.hIcon = icon as HICON;
    set_tip(&mut d, &tip_text());
    Shell_NotifyIconW(NIM_ADD, &d);
}

unsafe fn tray_refresh(hwnd: HWND) {
    let mut d = tray_data(hwnd);
    d.uFlags = NIF_TIP;
    set_tip(&mut d, &tip_text());
    Shell_NotifyIconW(NIM_MODIFY, &d);
}

unsafe fn tray_remove(hwnd: HWND) {
    let d = tray_data(hwnd);
    Shell_NotifyIconW(NIM_DELETE, &d);
}

unsafe fn tray_menu(hwnd: HWND) {
    let m = CreatePopupMenu();
    AppendMenuW(m, MF_STRING, CMD_SHOW, wide("Show Dyno").as_ptr());
    AppendMenuW(m, MF_STRING, CMD_CLEAN, wide("Clean memory now").as_ptr());
    AppendMenuW(m, MF_STRING, CMD_APPLY_SAFE, wide("Apply safe tweaks").as_ptr());
    AppendMenuW(m, MF_STRING, CMD_CHECK_UPDATE, wide("Check for updates").as_ptr());
    AppendMenuW(m, MF_SEPARATOR, 0, null());
    AppendMenuW(m, MF_STRING, CMD_EXIT, wide("Exit").as_ptr());
    let mut pt: POINT = zeroed();
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    TrackPopupMenu(m, TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, null());
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(m);
}

unsafe fn show(hwnd: HWND) {
    ShowWindow(hwnd, SW_SHOW);
    ShowWindow(hwnd, SW_RESTORE);
    SetForegroundWindow(hwnd);
    InvalidateRect(hwnd, null(), 0);
}

// ----------------------------------------------------------- background work
fn post_refresh(h: usize) {
    unsafe {
        PostMessageW(h as HWND, WM_APP_REFRESH, 0, 0);
    }
}

fn refresh_facts(h: usize) {
    std::thread::spawn(move || {
        let f = scan::facts();
        APP.lock().unwrap().facts = Some(f);
        post_refresh(h);
    });
}

fn start_clean() {
    let areas = {
        let mut a = APP.lock().unwrap();
        if a.busy_clean {
            return;
        }
        a.busy_clean = true;
        a.st.areas
    };
    let h = HWND_MAIN.load(Ordering::Relaxed);
    post_refresh(h);
    std::thread::spawn(move || {
        let (before, _) = mem::used();
        let _ = mem::clean(areas);
        std::thread::sleep(Duration::from_millis(500));
        let (after, total) = mem::used();
        {
            let mut a = APP.lock().unwrap();
            a.busy_clean = false;
            a.last_clean = Some(Instant::now());
            a.last_freed = before.saturating_sub(after);
            a.used = after;
            a.total = total;
        }
        post_refresh(h);
    });
}

fn start_apply(h: usize) {
    let sel: Vec<usize> = {
        let mut a = APP.lock().unwrap();
        let sel: Vec<usize> = (0..tweaks::COUNT).filter(|&i| a.st.tweaks[i]).collect();
        if sel.is_empty() {
            return;
        }
        settings::save(&a.st);
        let mut steps = Vec::new();
        for &i in &sel {
            steps.push(tweaks::TWEAKS[i].name.to_string());
        }
        a.apply_flags = vec![0; steps.len()];
        a.apply_steps = steps;
        a.apply_cur = 0;
        a.mode = UiMode::Applying;
        sel
    };
    CANCEL.store(false, Ordering::Relaxed);
    post_refresh(h);

    std::thread::spawn(move || {
        let set_cur = |k: usize| {
            APP.lock().unwrap().apply_cur = k;
            post_refresh(h);
        };
        let set_flag = |k: usize, ok: bool| {
            let mut a = APP.lock().unwrap();
            if k < a.apply_flags.len() {
                a.apply_flags[k] = if ok { 1 } else { 2 };
            }
            drop(a);
            post_refresh(h);
        };

        let used_before = mem::used().0;
        let temp_wanted = sel.iter().any(|&i| i >= 23 && i <= 29);
        let temp_before = if temp_wanted { scan::temp_total() } else { 0 };

        let mut results: Vec<StepResult> = Vec::new();
        let mut cancelled = false;
        for (k, &idx) in sel.iter().enumerate() {
            if CANCEL.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }
            set_cur(k);
            let ok = tweaks::apply(idx);
            let def = &tweaks::TWEAKS[idx];
            results.push(StepResult {
                name: def.name,
                ok,
                detail: if ok { def.done.to_string() } else { "Skipped. Couldn't apply this optimization".to_string() },
                reboot: def.reboot,
            });
            set_flag(k, ok);
            std::thread::sleep(Duration::from_millis(200));
        }
        set_cur(sel.len());
        std::thread::sleep(Duration::from_millis(400));

        let (used_after, total) = mem::used();
        let temp_after = if temp_wanted { scan::temp_total() } else { 0 };
        let applied = results.iter().filter(|r| r.ok).count();
        let skipped = results.len() - applied;
        let reboot_count = results.iter().filter(|r| r.ok && r.reboot).count();

        let title = if cancelled {
            format!("Stopped: {} of {} optimizations applied", applied, sel.len())
        } else {
            format!("{} of {} optimizations applied", applied, sel.len())
        };
        let mut sub = String::new();
        if skipped > 0 {
            sub.push_str(&format!("{} skipped. ", skipped));
        }
        sub.push_str("System optimization complete.");

        {
            let mut a = APP.lock().unwrap();
            a.results = results;
            a.res_title = title;
            a.res_sub = sub;
            a.freed_ram = used_before.saturating_sub(used_after);
            a.freed_temp = temp_before.saturating_sub(temp_after);
            a.reboot_count = reboot_count;
            a.used = used_after;
            a.total = total;
            a.mode = UiMode::Results;
        }
        post_refresh(h);
        refresh_facts(h);
    });
}

fn restart_now() {
    let _ = Command::new("shutdown")
        .args(["/r", "/t", "5", "/c", "Dyno: restarting to finish your changes"])
        .creation_flags(0x0800_0000)
        .spawn();
}

fn start_auto_update(hwnd: HWND, manual: bool) {
    if update::UPDATE_CHECK_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return;
    }

    let h_usize = hwnd as usize;
    std::thread::spawn(move || {
        let hwnd_local = h_usize as HWND;
        if manual {
            {
                let mut a = APP.lock().unwrap();
                a.update_step = 1;
                a.update_status = "Checking GitHub for updates...".to_string();
                a.update_detail = "Connecting to repository...".to_string();
                a.update_pct = 10.0;
                a.update_err = None;
                a.mode = UiMode::Updating;
            }
            unsafe {
                ShowWindow(hwnd_local, SW_SHOW);
                ShowWindow(hwnd_local, SW_RESTORE);
                SetForegroundWindow(hwnd_local);
                InvalidateRect(hwnd_local, null(), 0);
            }
        }

        match update::check_for_update() {
            Ok(Some(release)) => {
                {
                    let mut a = APP.lock().unwrap();
                    a.update_ver = release.version.clone();
                    a.update_step = 2; // downloading
                    a.update_status = format!("Downloading update v{}...", release.version);
                    a.update_detail = "Fetching update binary from GitHub...".to_string();
                    a.update_pct = 15.0;
                    a.update_err = None;
                    a.mode = UiMode::Updating;
                }
                unsafe {
                    ShowWindow(hwnd_local, SW_SHOW);
                    ShowWindow(hwnd_local, SW_RESTORE);
                    SetForegroundWindow(hwnd_local);
                    InvalidateRect(hwnd_local, null(), 0);
                }

                let dl_res = update::download_update(&release.download_url, move |pct, downloaded, total| {
                    let mut a = APP.lock().unwrap();
                    a.update_pct = pct;
                    if total > 0 {
                        a.update_detail = format!("Downloaded {} / {}", human(downloaded), human(total));
                    } else {
                        a.update_detail = format!("Downloaded {}", human(downloaded));
                    }
                    drop(a);
                    unsafe {
                        PostMessageW(hwnd_local, WM_APP_REFRESH, 0, 0);
                    }
                });

                match dl_res {
                    Ok(temp_file) => {
                        {
                            let mut a = APP.lock().unwrap();
                            a.update_step = 3; // installing
                            a.update_status = "Installing update & restarting...".to_string();
                            a.update_detail = "DynoCore is restarting into the new version...".to_string();
                            a.update_pct = 100.0;
                        }
                        unsafe {
                            InvalidateRect(hwnd_local, null(), 0);
                        }
                        std::thread::sleep(Duration::from_millis(800));

                        if let Err(e) = update::apply_update_and_restart(&temp_file, release.is_installer) {
                            let mut a = APP.lock().unwrap();
                            a.update_step = 4;
                            a.update_err = Some(format!("Failed to install update: {}", e));
                            drop(a);
                            unsafe {
                                InvalidateRect(hwnd_local, null(), 0);
                            }
                        }
                    }
                    Err(e) => {
                        let mut a = APP.lock().unwrap();
                        a.update_step = 4;
                        a.update_err = Some(format!("Download failed: {}", e));
                        drop(a);
                        unsafe {
                            InvalidateRect(hwnd_local, null(), 0);
                        }
                    }
                }
            }
            Ok(None) => {
                if manual {
                    {
                        let mut a = APP.lock().unwrap();
                        a.update_step = 3;
                        a.update_status = "DynoCore is up to date!".to_string();
                        a.update_detail = format!("v{} is the latest version available.", env!("CARGO_PKG_VERSION"));
                        a.update_pct = 100.0;
                    }
                    unsafe {
                        InvalidateRect(hwnd_local, null(), 0);
                    }
                    std::thread::sleep(Duration::from_millis(1500));
                    {
                        let mut a = APP.lock().unwrap();
                        if a.mode == UiMode::Updating {
                            a.mode = UiMode::Dashboard;
                        }
                    }
                    unsafe {
                        InvalidateRect(hwnd_local, null(), 0);
                    }
                }
            }
            Err(e) => {
                if manual {
                    let mut a = APP.lock().unwrap();
                    a.update_step = 4;
                    a.update_err = Some(format!("Error checking updates: {}", e));
                    drop(a);
                    unsafe {
                        InvalidateRect(hwnd_local, null(), 0);
                    }
                }
            }
        }

        update::UPDATE_CHECK_IN_PROGRESS.store(false, Ordering::SeqCst);
    });
}

unsafe fn on_timer(hwnd: HWND) {
    let (used, total) = mem::used();
    let (want_auto, tick) = {
        let mut a = APP.lock().unwrap();
        a.used = used;
        a.total = total;
        a.ticks = a.ticks.wrapping_add(1);
        let cooled = a.last_clean.map_or(true, |t| t.elapsed() >= COOLDOWN);
        (a.st.auto && !a.busy_clean && cooled && percent(used, total) >= a.st.thr as f64, a.ticks)
    };
    if want_auto {
        start_clean();
    }
    if tick % 5 == 0 {
        tray_refresh(hwnd);
    }
    if tick > 0 && tick % 14400 == 0 {
        start_auto_update(hwnd, false);
    }
    if IsWindowVisible(hwnd) != 0 {
        InvalidateRect(hwnd, null(), 0);
    }
}

// ---------------------------------------------------------------- click handler
unsafe fn on_click(hwnd: HWND, lparam: LPARAM) {
    let s = scale(hwnd);
    let x = (((lparam & 0xFFFF) as i16) as f32 / s) as i32;
    let y = ((((lparam >> 16) & 0xFFFF) as i16) as f32 / s) as i32;
    let h = hwnd as usize;
    let mode = APP.lock().unwrap().mode;

    match mode {
        UiMode::Updating => {
            let has_err = APP.lock().unwrap().update_err.is_some();
            if has_err {
                if UPDATE_RETRY_BTN.contains(x, y) {
                    start_auto_update(hwnd, true);
                } else if UPDATE_CANCEL_BTN.contains(x, y) {
                    APP.lock().unwrap().mode = UiMode::Dashboard;
                    InvalidateRect(hwnd, null(), 0);
                }
            }
        }
        UiMode::Confirm => {
            if CONFIRM_YES_BTN.contains(x, y) {
                start_apply(h);
            } else if CONFIRM_NO_BTN.contains(x, y) {
                APP.lock().unwrap().mode = UiMode::Dashboard;
                InvalidateRect(hwnd, null(), 0);
            }
        }
        UiMode::Applying => {
            if CANCEL_BTN.contains(x, y) {
                CANCEL.store(true, Ordering::Relaxed);
            }
        }
        UiMode::Results => {
            let reboot = APP.lock().unwrap().reboot_count > 0;
            if reboot {
                if RESTART_NOW_BTN.contains(x, y) {
                    restart_now();
                } else if RESTART_LATER_BTN.contains(x, y) {
                    APP.lock().unwrap().mode = UiMode::Dashboard;
                }
            } else if DONE_BTN.contains(x, y) {
                APP.lock().unwrap().mode = UiMode::Dashboard;
            }
            InvalidateRect(hwnd, null(), 0);
        }
        UiMode::Dashboard => {
            if CLEAN_BTN.contains(x, y) {
                start_clean();
                return;
            }
            if y >= TAB_Y && y < TAB_Y + TAB_H && x >= 21 && x < 519 {
                let t = (((x - 21) / 83) as usize).min(tweaks::CAT_NAMES.len() - 1);
                let mut a = APP.lock().unwrap();
                a.active_tab = t;
                a.scroll_y = 0;
                drop(a);
                InvalidateRect(hwnd, null(), 0);
                return;
            }
            let (tab, scroll_y) = {
                let a = APP.lock().unwrap();
                (a.active_tab, a.scroll_y)
            };
            for (n, i) in (tweaks::CAT_START[tab]..tweaks::CAT_END[tab]).enumerate() {
                let ty = LIST_Y + n as i32 * ROW_STEP - scroll_y;
                if ty >= LIST_Y && ty + ROW_H <= FOOTER_Y && R::new(20, ty, 520, ty + ROW_H).contains(x, y) {
                    let mut a = APP.lock().unwrap();
                    a.st.tweaks[i] = !a.st.tweaks[i];
                    settings::save(&a.st);
                    drop(a);
                    InvalidateRect(hwnd, null(), 0);
                    return;
                }
            }
            if RECO_BTN.contains(x, y) {
                let mut a = APP.lock().unwrap();
                for i in 0..tweaks::COUNT {
                    a.st.tweaks[i] = tweaks::TWEAKS[i].risk == tweaks::SAFE;
                }
                settings::save(&a.st);
                drop(a);
                InvalidateRect(hwnd, null(), 0);
            } else if APPLY_BTN.contains(x, y) {
                let sel_count = APP.lock().unwrap().st.tweaks.iter().filter(|t| **t).count();
                if sel_count > 0 {
                    APP.lock().unwrap().mode = UiMode::Confirm;
                    InvalidateRect(hwnd, null(), 0);
                }
            }
        }
    }
}

// ------------------------------------------------------------------ window proc
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONDOWN => {
            on_click(hwnd, lparam);
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as i16) as i32;
            let mut a = APP.lock().unwrap();
            if a.mode == UiMode::Dashboard {
                let tab = a.active_tab;
                let count = (tweaks::CAT_END[tab] - tweaks::CAT_START[tab]) as i32;
                let total_h = count * ROW_STEP;
                let view_h = FOOTER_Y - LIST_Y;
                let max_scroll = (total_h - view_h + 10).max(0);
                if max_scroll > 0 {
                    let step = if delta > 0 { -ROW_STEP } else { ROW_STEP };
                    a.scroll_y = (a.scroll_y + step).clamp(0, max_scroll);
                    drop(a);
                    InvalidateRect(hwnd, null(), 0);
                }
            }
            0
        }
        WM_TIMER => {
            on_timer(hwnd);
            0
        }
        WM_APP_REFRESH => {
            InvalidateRect(hwnd, null(), 0);
            tray_refresh(hwnd);
            0
        }
        WM_APP_TRAY => {
            match lparam as u32 {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => show(hwnd),
                WM_RBUTTONUP => tray_menu(hwnd),
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            match wparam & 0xFFFF {
                CMD_SHOW => show(hwnd),
                CMD_CLEAN => start_clean(),
                CMD_APPLY_SAFE => apply_safe_tweaks_silently(),
                CMD_CHECK_UPDATE => start_auto_update(hwnd, true),
                CMD_EXIT => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_DPICHANGED => {
            let r = &*(lparam as *const RECT);
            SetWindowPos(hwnd, 0 as _, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            make_fonts(hwnd);
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_CLOSE => {
            ShowWindow(hwnd, SW_HIDE);
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, ID_TIMER);
            tray_remove(hwnd);
            PostQuitMessage(0);
            0
        }
        _ => {
            let tb = TASKBAR_CREATED.load(Ordering::Relaxed);
            if tb != 0 && msg == tb {
                tray_add(hwnd);
                0
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
    }
}

fn ensure_autostart() {
    if let Ok(exe_path) = std::env::current_exe() {
        let exe_str = exe_path.to_string_lossy();
        let cmd = format!("\"{}\" --tray", exe_str);

        // 1. Task Scheduler: Run at logon with highest privileges (no UAC prompt on boot!)
        let _ = Command::new("schtasks")
            .args(&["/create", "/tn", "Dyno", "/tr", &cmd, "/sc", "onlogon", "/rl", "highest", "/f"])
            .creation_flags(0x08000000)
            .output();

        // 2. Also register HKCU Run key as standard fallback
        let _ = Command::new("reg")
            .args(&["add", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", "/v", "Dyno", "/t", "REG_SZ", "/d", &cmd, "/f"])
            .creation_flags(0x08000000)
            .output();
    }
}

fn remove_autostart() {
    let _ = Command::new("schtasks")
        .args(&["/delete", "/tn", "Dyno", "/f"])
        .creation_flags(0x08000000)
        .output();
    let _ = Command::new("reg")
        .args(&["delete", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", "/v", "Dyno", "/f"])
        .creation_flags(0x08000000)
        .output();
}

fn apply_safe_tweaks_silently() {
    std::thread::spawn(|| {
        for i in 0..tweaks::COUNT {
            if tweaks::TWEAKS[i].risk == tweaks::SAFE {
                let _ = tweaks::apply(i);
            }
        }
        let h = HWND_MAIN.load(Ordering::Relaxed);
        if h != 0 {
            refresh_facts(h);
        }
    });
}

// ------------------------------------------------------------------------ main
fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--update-cleanup") {
        update::cleanup_previous_update();
    }

    if args.iter().any(|a| a == "--clean") {
        let (before, total) = mem::used();
        let _ = mem::clean([true; 4]);
        let (after, _) = mem::used();
        let freed = before.saturating_sub(after);
        println!("Freed {} of RAM. In use: {} / {}", human(freed), human(after), human(total));
        return;
    }
    if args.iter().any(|a| a == "--autostart-off") {
        remove_autostart();
        return;
    }
    if args.iter().any(|a| a == "--autostart-on") {
        ensure_autostart();
        return;
    }
    if args.iter().any(|a| a == "--apply-safe-only") {
        for i in 0..tweaks::COUNT {
            if tweaks::TWEAKS[i].risk == tweaks::SAFE {
                let _ = tweaks::apply(i);
            }
        }
        return;
    }

    // 1. Single instance check
    let _single = unsafe { CreateMutexW(null(), 0, wide("Dyno.SingleInstance").as_ptr()) };
    unsafe {
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let h = FindWindowW(wide(CLASS).as_ptr(), null());
            if (h as usize) != 0 {
                show(h);
            }
            return;
        }
    }

    // 2. Check elevation to bypass UAC prompts via Task Scheduler
    if !mem::is_elevated() {
        let ran_task = Command::new("schtasks")
            .args(&["/run", "/tn", "Dyno"])
            .creation_flags(0x08000000)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if ran_task {
            // Elevated instance launched via Task Scheduler without UAC prompt!
            if !args.iter().any(|a| a == "--tray" || a == "--silent" || a == "--background" || a == "--autostart") {
                for _ in 0..25 {
                    std::thread::sleep(Duration::from_millis(100));
                    unsafe {
                        let h = FindWindowW(wide(CLASS).as_ptr(), null());
                        if (h as usize) != 0 {
                            show(h);
                            break;
                        }
                    }
                }
            }
            return;
        } else {
            // Task does not exist yet (first-time portable launch). Elevate once to set up task.
            unsafe {
                if let Ok(exe_path) = std::env::current_exe() {
                    let exe_w = wide(&exe_path.to_string_lossy());
                    let params_str = if args.iter().any(|a| a == "--tray") { "--tray" } else { "" };
                    let params_w = wide(params_str);
                    ShellExecuteW(
                        0,
                        wide("runas").as_ptr(),
                        exe_w.as_ptr(),
                        params_w.as_ptr(),
                        null(),
                        SW_SHOWNORMAL,
                    );
                }
            }
            return;
        }
    }

    std::thread::spawn(ensure_autostart);

    unsafe {
        let hinst = GetModuleHandleW(null());

        {
            let mut a = APP.lock().unwrap();
            a.st = settings::load();
            let (u, t) = mem::used();
            a.used = u;
            a.total = t;
        }

        // The Dyno icon from the exe's resources (dyno.ico = ID 1).
        let mut icon_big = LoadImageW(hinst, 1usize as PCWSTR, IMAGE_ICON, GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON), LR_DEFAULTCOLOR);
        let mut icon_small = LoadImageW(hinst, 1usize as PCWSTR, IMAGE_ICON, GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON), LR_DEFAULTCOLOR);
        if icon_big == 0 {
            icon_big = LoadIconW(0 as _, IDI_APPLICATION);
        }
        if icon_small == 0 {
            icon_small = LoadIconW(0 as _, IDI_APPLICATION);
        }
        APP.lock().unwrap().icon = icon_small as usize;

        let class = wide(CLASS);
        let mut wc: WNDCLASSW = zeroed();
        wc.style = CS_HREDRAW | CS_VREDRAW;
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = hinst;
        wc.hIcon = icon_big as HICON;
        wc.hCursor = LoadCursorW(0 as _, IDC_ARROW);
        wc.lpszClassName = class.as_ptr();
        RegisterClassW(&wc);
        TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Ordering::Relaxed);

        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("Dyno").as_ptr(),
            STYLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WIN_W,
            WIN_H,
            0 as _,
            0 as _,
            hinst,
            null(),
        );
        if (hwnd as usize) == 0 {
            return;
        }
        HWND_MAIN.store(hwnd as usize, Ordering::Relaxed);

        SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, icon_big as isize);
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, icon_small as isize);

        style_titlebar(hwnd);
        fit_window(hwnd);
        make_fonts(hwnd);
        tray_add(hwnd);
        SetTimer(hwnd, ID_TIMER, 1000, None);

        // Read this PC's real state in the background so the "Now:" lines are accurate.
        refresh_facts(hwnd as usize);

        // Auto-apply safe tweaks silently in background (no commands required)
        apply_safe_tweaks_silently();

        // Immediately purge RAM & standby list on launch so everything starts 100% clean
        start_clean();

        let hidden = args.iter().any(|a| a == "--tray" || a == "--silent" || a == "--background" || a == "--autostart");
        if !hidden {
            ShowWindow(hwnd, SW_SHOW);
            UpdateWindow(hwnd);
        }

        // Auto-check for updates 3 seconds after startup in background
        let h_update = hwnd as usize;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(3));
            start_auto_update(h_update as HWND, false);
        });

        let mut m: MSG = zeroed();
        while GetMessageW(&mut m, 0 as _, 0, 0) > 0 {
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
    }
}
