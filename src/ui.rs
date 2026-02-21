//! # UI 模块 — 候选词窗口
//!
//! 无边框、置顶、半透明的候选词悬浮窗口。
//! 基于 Win32 GDI，傻瓜化设计，零配置。

use std::ffi::c_void;
use std::sync::Once;
use log::info;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

// ============================================================
// 视觉常量 — 拒绝 YAML，硬编码即正义
// ============================================================

/// RGB → Win32 COLORREF (BGR 格式)
const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((b as u32) << 16 | (g as u32) << 8 | r as u32)
}

// 配色方案 — 深灰 + 亮蓝高亮
const BG: COLORREF = rgb(46, 49, 62);          // #2E313E 深灰背景
const TEXT_CLR: COLORREF = rgb(200, 204, 216);  // #C8CCD8 柔白文字
const INDEX_CLR: COLORREF = rgb(130, 134, 150); // #828696 灰色序号
const HL_BG: COLORREF = rgb(122, 162, 247);     // #7AA2F7 亮蓝高亮
const HL_TEXT: COLORREF = rgb(255, 255, 255);   // #FFFFFF 高亮文字纯白

// 排版参数
const FONT_SZ: i32 = 24;          // 候选字体大小
const IDX_FONT_SZ: i32 = 16;      // 序号字体大小
const WIN_ALPHA: u8 = 240;         // 窗口透明度
const PAD_H: i32 = 18;             // 水平内边距
const PAD_V: i32 = 12;             // 垂直内边距
const ITEM_GAP: i32 = 28;          // 候选词间距
const HL_PAD_H: i32 = 8;           // 高亮水平内边距
const HL_PAD_V: i32 = 4;           // 高亮垂直内边距
const HL_RADIUS: i32 = 8;          // 高亮圆角半径
const WIN_RADIUS: i32 = 14;        // 窗口圆角半径

const WND_CLASS: PCWSTR = w!("AiPinyinCandidate");
static REGISTER_ONCE: Once = Once::new();

// ============================================================
// WindowState — 通过 GWLP_USERDATA 附着在窗口上
// ============================================================

struct WindowState {
    candidates: Vec<String>,
    selected: usize,
    font: HFONT,
    font_idx: HFONT,
}

impl WindowState {
    fn new() -> Self {
        unsafe {
            let font = CreateFontW(
                FONT_SZ, 0, 0, 0, FW_MEDIUM.0 as i32, 0, 0, 0,
                DEFAULT_CHARSET.0 as u32, OUT_TT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32, DEFAULT_PITCH.0 as u32, w!("微软雅黑"),
            );
            let font_idx = CreateFontW(
                IDX_FONT_SZ, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0,
                DEFAULT_CHARSET.0 as u32, OUT_TT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32, DEFAULT_PITCH.0 as u32, w!("微软雅黑"),
            );
            Self { candidates: vec![], selected: 0, font, font_idx }
        }
    }
}

impl Drop for WindowState {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.font);
            let _ = DeleteObject(self.font_idx);
        }
    }
}

// ============================================================
// CandidateWindow — 公开 API
// ============================================================

/// 候选词悬浮窗口
pub struct CandidateWindow {
    hwnd: HWND,
    state: *mut WindowState,
}

// CandidateWindow 只在创建线程使用，手动声明 Send
unsafe impl Send for CandidateWindow {}

impl CandidateWindow {
    /// 创建候选词窗口（初始隐藏）
    pub fn new() -> Result<Self> {
        register_class()?;

        let state = Box::new(WindowState::new());
        let state_ptr = Box::into_raw(state);

        let hwnd = unsafe {
            let hinstance = GetModuleHandleW(None)?;
            let ex_style = WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE;

            let hinstance_val: HINSTANCE = hinstance.into();
            let hwnd = CreateWindowExW(
                ex_style,
                WND_CLASS,
                w!("AiPinyin"),
                WS_POPUP,
                0, 0, 200, 50,
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

    /// 更新候选词列表并自动调整窗口尺寸
    pub fn draw_candidates(&self, candidates: &[&str]) {
        unsafe {
            let state = &mut *self.state;
            state.candidates = candidates.iter().map(|s| s.to_string()).collect();
            state.selected = 0;

            // 计算所需窗口尺寸
            let hdc = GetDC(self.hwnd);
            let (w, h) = calc_size(hdc, state);
            ReleaseDC(self.hwnd, hdc);

            let _ = SetWindowPos(
                self.hwnd, None, 0, 0, w, h,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );

            // 裁剪窗口为圆角矩形
            let rgn = CreateRoundRectRgn(0, 0, w, h, WIN_RADIUS, WIN_RADIUS);
            SetWindowRgn(self.hwnd, rgn, TRUE);

            let _ = InvalidateRect(self.hwnd, None, TRUE);
        }
    }

    /// 在指定屏幕坐标显示窗口
    pub fn show(&self, x: i32, y: i32) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd, HWND_TOPMOST, x, y, 0, 0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    /// 隐藏窗口
    pub fn hide(&self) {
        unsafe { let _ = ShowWindow(self.hwnd, SW_HIDE); }
    }

    /// 移动窗口到光标附近位置
    pub fn set_position(&self, x: i32, y: i32) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd, None, x, y, 0, 0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    /// 设置选中项
    pub fn select(&self, index: usize) {
        unsafe {
            let state = &mut *self.state;
            if index < state.candidates.len() {
                state.selected = index;
                let _ = InvalidateRect(self.hwnd, None, TRUE);
            }
        }
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

/// 运行 Win32 消息循环（阻塞直到收到 WM_QUIT）
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
            if atom == 0 {
                result = Err(Error::from_win32());
            }
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
            if !ptr.is_null() {
                paint(hdc, hwnd, &*ptr);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1), // 我们自己画背景
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

/// 绘制候选词列表
unsafe fn paint(hdc: HDC, hwnd: HWND, state: &WindowState) {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);

    // 背景填充
    let bg_brush = CreateSolidBrush(BG);
    FillRect(hdc, &rc, bg_brush);
    let _ = DeleteObject(bg_brush);

    if state.candidates.is_empty() { return; }

    SetBkMode(hdc, TRANSPARENT);

    let mut x = PAD_H;
    let y_mid = (rc.bottom - rc.top) / 2;

    for (i, cand) in state.candidates.iter().enumerate() {
        let is_sel = i == state.selected;

        // 1) 绘制序号 "1."
        SelectObject(hdc, state.font_idx);
        SetTextColor(hdc, if is_sel { HL_TEXT } else { INDEX_CLR });

        let idx_str = format!("{}.", i + 1);
        let idx_w: Vec<u16> = idx_str.encode_utf16().collect();
        let mut sz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &idx_w, &mut sz);

        let idx_y = y_mid - sz.cy / 2;
        let _ = TextOutW(hdc, x, idx_y, &idx_w);
        x += sz.cx + 2;

        // 2) 测量候选字
        SelectObject(hdc, state.font);
        let cand_w: Vec<u16> = cand.encode_utf16().collect();
        let mut csz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &cand_w, &mut csz);

        let text_y = y_mid - csz.cy / 2;

        // 3) 高亮背景
        if is_sel {
            let hl_rc = RECT {
                left: x - HL_PAD_H,
                top: text_y - HL_PAD_V,
                right: x + csz.cx + HL_PAD_H,
                bottom: text_y + csz.cy + HL_PAD_V,
            };
            let hl_brush = CreateSolidBrush(HL_BG);
            let old_brush = SelectObject(hdc, hl_brush);
            let null_pen = GetStockObject(NULL_PEN);
            let old_pen = SelectObject(hdc, null_pen);
            let _ = RoundRect(hdc, hl_rc.left, hl_rc.top, hl_rc.right, hl_rc.bottom,
                      HL_RADIUS, HL_RADIUS);
            SelectObject(hdc, old_pen);
            SelectObject(hdc, old_brush);
            let _ = DeleteObject(hl_brush);
        }

        // 4) 绘制候选字
        SetTextColor(hdc, if is_sel { HL_TEXT } else { TEXT_CLR });
        let _ = TextOutW(hdc, x, text_y, &cand_w);

        x += csz.cx + ITEM_GAP;
    }
}

/// 根据候选词计算所需窗口尺寸
unsafe fn calc_size(hdc: HDC, state: &WindowState) -> (i32, i32) {
    if state.candidates.is_empty() {
        return (0, 0);
    }

    let old_font = SelectObject(hdc, state.font);
    let mut total_w: i32 = PAD_H * 2;
    let mut max_h: i32 = 0;

    for (i, cand) in state.candidates.iter().enumerate() {
        // 测量序号
        SelectObject(hdc, state.font_idx);
        let idx_str = format!("{}.", i + 1);
        let idx_w: Vec<u16> = idx_str.encode_utf16().collect();
        let mut sz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &idx_w, &mut sz);
        total_w += sz.cx + 2;

        // 测量候选字
        SelectObject(hdc, state.font);
        let cand_w: Vec<u16> = cand.encode_utf16().collect();
        let mut csz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &cand_w, &mut csz);
        total_w += csz.cx + ITEM_GAP;

        if csz.cy > max_h { max_h = csz.cy; }
    }

    total_w -= ITEM_GAP; // 最后一个不需要间距
    let h = max_h + PAD_V * 2 + HL_PAD_V * 2;

    SelectObject(hdc, old_font);
    (total_w, h)
}
