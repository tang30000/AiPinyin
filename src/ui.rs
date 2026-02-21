//! # UI 模块 — 候选词窗口（双排布局）
//!
//! 上排：拼音输入（小灰字）
//! 下排：数字序号 + 候选汉字
//!
//! 无边框、置顶、半透明，圆角矩形。

use std::ffi::c_void;
use std::sync::Once;
use log::info;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

// ============================================================
// 视觉常量
// ============================================================

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((b as u32) << 16 | (g as u32) << 8 | r as u32)
}

// 配色
const BG: COLORREF        = rgb(46, 49, 62);    // #2E313E 深灰背景
const PINYIN_CLR: COLORREF= rgb(110, 115, 140); // 上排拼音小字颜色（暗灰蓝）
const TEXT_CLR: COLORREF  = rgb(200, 204, 216); // 候选文字
const INDEX_CLR: COLORREF = rgb(130, 134, 150); // 序号颜色
const HL_BG: COLORREF     = rgb(122, 162, 247); // #7AA2F7 高亮背景
const HL_TEXT: COLORREF   = rgb(255, 255, 255); // 高亮文字

// 字号
const PINYIN_FONT_SZ: i32 = 13;  // 上排拼音小字
const CAND_FONT_SZ: i32   = 20;  // 下排候选汉字
const IDX_FONT_SZ: i32    = 20;  // 序号（和候选同大）

// 间距
const WIN_ALPHA: u8  = 242;
const PAD_H: i32     = 14;  // 左右内边距
const PAD_TOP: i32   = 7;   // 顶部内边距
const PAD_BOT: i32   = 8;   // 底部内边距
const ROW_GAP: i32   = 3;   // 两排之间的间隔
const ITEM_GAP: i32  = 22;  // 候选词之间间距
const HL_PAD_H: i32  = 7;   // 高亮水平内边距
const HL_PAD_V: i32  = 3;   // 高亮垂直内边距
const HL_RADIUS: i32 = 7;   // 高亮圆角
const WIN_RADIUS: i32= 14;  // 窗口圆角

const WND_CLASS: PCWSTR = w!("AiPinyinCandidate");
static REGISTER_ONCE: Once = Once::new();

// ============================================================
// WindowState
// ============================================================

struct WindowState {
    raw_input: String,       // 上排拼音原文
    candidates: Vec<String>, // 下排候选词
    selected: usize,
    font_pinyin: HFONT,      // 拼音小字字体
    font_cand: HFONT,        // 候选汉字字体
    font_idx: HFONT,         // 序号字体
}

impl WindowState {
    fn new() -> Self {
        unsafe {
            let mk_font = |sz: i32, weight: i32| CreateFontW(
                sz, 0, 0, 0, weight, 0, 0, 0,
                DEFAULT_CHARSET.0 as u32, OUT_TT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32,
                DEFAULT_PITCH.0 as u32, w!("微软雅黑"),
            );
            Self {
                raw_input: String::new(),
                candidates: vec![],
                selected: 0,
                font_pinyin: mk_font(PINYIN_FONT_SZ, FW_NORMAL.0 as i32),
                font_cand:   mk_font(CAND_FONT_SZ,   FW_MEDIUM.0 as i32),
                font_idx:    mk_font(IDX_FONT_SZ,    FW_NORMAL.0 as i32),
            }
        }
    }
}

impl Drop for WindowState {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.font_pinyin);
            let _ = DeleteObject(self.font_cand);
            let _ = DeleteObject(self.font_idx);
        }
    }
}

// ============================================================
// CandidateWindow — 公开 API
// ============================================================

pub struct CandidateWindow {
    hwnd: HWND,
    state: *mut WindowState,
}

unsafe impl Send for CandidateWindow {}

impl CandidateWindow {
    pub fn new() -> Result<Self> {
        register_class()?;

        let state = Box::new(WindowState::new());
        let state_ptr = Box::into_raw(state);

        let hwnd = unsafe {
            let hinstance = GetModuleHandleW(None)?;
            let hinstance_val: HINSTANCE = hinstance.into();
            let ex_style = WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;

            let hwnd = CreateWindowExW(
                ex_style,
                WND_CLASS,
                w!("AiPinyin"),
                WS_POPUP,
                0, 0, 300, 60,
                None, None,
                hinstance_val,
                Some(state_ptr as *const c_void),
            )?;

            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), WIN_ALPHA, LWA_ALPHA);
            hwnd
        };

        info!("[UI] 候选词窗口已创建");
        Ok(Self { hwnd, state: state_ptr })
    }

    /// 更新候选词列表并重绘（同时传入当前拼音原文）
    pub fn draw_candidates(&self, candidates: &[&str]) {
        unsafe {
            let state = &mut *self.state;
            state.candidates = candidates.iter().map(|s| s.to_string()).collect();
            state.selected = 0;
            self.resize_and_redraw(state);
        }
    }

    /// 更新拼音原文（上排小字）
    pub fn set_raw_input(&self, raw: &str) {
        unsafe {
            let state = &mut *self.state;
            state.raw_input = raw.to_string();
        }
    }

    /// 一站式更新：设置拼音 + 候选词 + 定位光标 + 显示
    pub fn update_candidates(&self, raw: &str, candidates: &[&str]) {
        if candidates.is_empty() {
            self.hide();
            return;
        }
        unsafe {
            let state = &mut *self.state;
            state.raw_input = raw.to_string();
            state.candidates = candidates.iter().map(|s| s.to_string()).collect();
            state.selected = 0;
            self.resize_and_redraw(state);
        }
    }

    /// 在指定屏幕坐标显示
    pub fn show(&self, x: i32, y: i32) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd, HWND_TOPMOST, x, y, 0, 0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    /// 隐藏窗口，同时清空状态
    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            let state = &mut *self.state;
            state.raw_input.clear();
            state.candidates.clear();
        }
    }

    pub fn set_position(&self, x: i32, y: i32) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd, None, x, y, 0, 0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    pub fn select(&self, index: usize) {
        unsafe {
            let state = &mut *self.state;
            if index < state.candidates.len() {
                state.selected = index;
                let _ = InvalidateRect(self.hwnd, None, TRUE);
            }
        }
    }

    /// 根据 GetGUIThreadInfo 显示候选窗口在光标下方
    pub fn show_at_caret(&self) {
        unsafe {
            let fg = GetForegroundWindow();
            let mut pt = POINT::default();
            let mut ok = false;

            if !fg.is_invalid() {
                let tid = GetWindowThreadProcessId(fg, None);
                let mut gi = GUITHREADINFO {
                    cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                    ..Default::default()
                };
                if GetGUIThreadInfo(tid, &mut gi).is_ok() && !gi.hwndCaret.is_invalid() {
                    pt = POINT { x: gi.rcCaret.left, y: gi.rcCaret.bottom };
                    let _ = ClientToScreen(gi.hwndCaret, &mut pt);
                    if pt.x > 0 && pt.y > 0 { ok = true; }
                }
            }
            if !ok {
                let _ = GetCursorPos(&mut pt);
                pt.y += 24;
            }
            self.show(pt.x, pt.y + 4);
        }
    }

    // ── 内部：调整尺寸 + 重绘 ──
    unsafe fn resize_and_redraw(&self, state: &WindowState) {
        let hdc = GetDC(self.hwnd);
        let (w, h) = calc_size(hdc, state);
        ReleaseDC(self.hwnd, hdc);

        let _ = SetWindowPos(
            self.hwnd, None, 0, 0, w, h,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );

        let rgn = CreateRoundRectRgn(0, 0, w, h, WIN_RADIUS, WIN_RADIUS);
        SetWindowRgn(self.hwnd, rgn, TRUE);

        let _ = InvalidateRect(self.hwnd, None, TRUE);
    }
}

impl Drop for CandidateWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            let _ = Box::from_raw(self.state);
        }
    }
}

// ============================================================
// 消息循环
// ============================================================

pub fn run_message_loop() {
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

// ============================================================
// 内部实现
// ============================================================

fn register_class() -> Result<()> {
    let mut result: Result<()> = Ok(());
    REGISTER_ONCE.call_once(|| {
        unsafe {
            let hinstance = match GetModuleHandleW(None) {
                Ok(h) => h,
                Err(e) => { result = Err(e); return; }
            };
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: WND_CLASS,
                hCursor: LoadCursorW(None, IDC_ARROW).ok().unwrap_or_default(),
                ..Default::default()
            };
            let atom = RegisterClassExW(&wc);
            if atom == 0 { result = Err(Error::from_win32()); }
        }
    });
    result
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;
            if !ptr.is_null() { paint(hdc, hwnd, &*ptr); }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_KEYDOWN if wparam.0 == 0x1B => { // ESC
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ============================================================
// 绘制：双排布局
// ============================================================

unsafe fn paint(hdc: HDC, hwnd: HWND, state: &WindowState) {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);

    // ── 背景 ──
    let bg_brush = CreateSolidBrush(BG);
    FillRect(hdc, &rc, bg_brush);
    let _ = DeleteObject(bg_brush);

    if state.candidates.is_empty() { return; }

    SetBkMode(hdc, TRANSPARENT);

    // ── 上排：拼音原文 ──
    if !state.raw_input.is_empty() {
        SelectObject(hdc, state.font_pinyin);
        SetTextColor(hdc, PINYIN_CLR);
        let w: Vec<u16> = state.raw_input.encode_utf16().collect();
        let _ = TextOutW(hdc, PAD_H, PAD_TOP, &w);
    }

    // ── 下排：候选词 ──
    // 计算上排高度，用于定位下排
    let pinyin_h = pinyin_row_height(hdc, state);
    let y_cand = PAD_TOP + pinyin_h + ROW_GAP;
    let y_mid  = y_cand + (cand_row_height(hdc, state)) / 2;

    let mut x = PAD_H;

    for (i, cand) in state.candidates.iter().enumerate() {
        let is_sel = i == state.selected;

        // 序号
        SelectObject(hdc, state.font_idx);
        SetTextColor(hdc, if is_sel { HL_TEXT } else { INDEX_CLR });
        let idx_str = format!("{}.", i + 1);
        let idx_w: Vec<u16> = idx_str.encode_utf16().collect();
        let mut isz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &idx_w, &mut isz);
        let _ = TextOutW(hdc, x, y_mid - isz.cy / 2, &idx_w);
        x += isz.cx + 2;

        // 候选字尺寸
        SelectObject(hdc, state.font_cand);
        let cw: Vec<u16> = cand.encode_utf16().collect();
        let mut csz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &cw, &mut csz);
        let text_y = y_mid - csz.cy / 2;

        // 高亮背景
        if is_sel {
            let hl_rc = RECT {
                left:   x - HL_PAD_H,
                top:    text_y - HL_PAD_V,
                right:  x + csz.cx + HL_PAD_H,
                bottom: text_y + csz.cy + HL_PAD_V,
            };
            let hl_brush = CreateSolidBrush(HL_BG);
            let ob = SelectObject(hdc, hl_brush);
            let op = SelectObject(hdc, GetStockObject(NULL_PEN));
            let _ = RoundRect(hdc, hl_rc.left, hl_rc.top, hl_rc.right, hl_rc.bottom,
                              HL_RADIUS, HL_RADIUS);
            SelectObject(hdc, op);
            SelectObject(hdc, ob);
            let _ = DeleteObject(hl_brush);
        }

        // 候选字
        SetTextColor(hdc, if is_sel { HL_TEXT } else { TEXT_CLR });
        let _ = TextOutW(hdc, x, text_y, &cw);
        x += csz.cx + ITEM_GAP;
    }
}

// ── 上排拼音高度 ──
unsafe fn pinyin_row_height(hdc: HDC, state: &WindowState) -> i32 {
    if state.raw_input.is_empty() { return 0; }
    let old = SelectObject(hdc, state.font_pinyin);
    let w: Vec<u16> = state.raw_input.encode_utf16().collect();
    let mut sz = SIZE::default();
    let _ = GetTextExtentPoint32W(hdc, &w, &mut sz);
    SelectObject(hdc, old);
    sz.cy
}

// ── 下排候选词的行高 ──
unsafe fn cand_row_height(hdc: HDC, state: &WindowState) -> i32 {
    let old = SelectObject(hdc, state.font_cand);
    let sample: Vec<u16> = "汉".encode_utf16().collect();
    let mut sz = SIZE::default();
    let _ = GetTextExtentPoint32W(hdc, &sample, &mut sz);
    SelectObject(hdc, old);
    sz.cy + HL_PAD_V * 2
}

// ── 窗口整体尺寸 ──
unsafe fn calc_size(hdc: HDC, state: &WindowState) -> (i32, i32) {
    if state.candidates.is_empty() { return (0, 0); }

    // 宽度：遍历所有候选词
    let mut total_w = PAD_H * 2;
    for (i, cand) in state.candidates.iter().enumerate() {
        SelectObject(hdc, state.font_idx);
        let idx_str = format!("{}.", i + 1);
        let iw: Vec<u16> = idx_str.encode_utf16().collect();
        let mut isz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &iw, &mut isz);
        total_w += isz.cx + 2;

        SelectObject(hdc, state.font_cand);
        let cw: Vec<u16> = cand.encode_utf16().collect();
        let mut csz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &cw, &mut csz);
        total_w += csz.cx + ITEM_GAP;
    }
    total_w -= ITEM_GAP; // 最后一项不需要间距

    // 也要考虑上排拼音宽度
    if !state.raw_input.is_empty() {
        SelectObject(hdc, state.font_pinyin);
        let pw: Vec<u16> = state.raw_input.encode_utf16().collect();
        let mut psz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &pw, &mut psz);
        total_w = total_w.max(psz.cx + PAD_H * 2);
    }

    // 高度：上排 + 间隔 + 下排 + 上下内边距
    let ph = pinyin_row_height(hdc, state);
    let ch = cand_row_height(hdc, state);
    let h = PAD_TOP + ph + if ph > 0 { ROW_GAP } else { 0 } + ch + PAD_BOT;

    (total_w, h)
}
