//! # UI 模块 — 候选词窗口（双排布局）
//!
//! 上排：拼音输入（小灰字）
//! 下排：数字序号 + 候选汉字
//!
//! 右上角：[JS] 按鈕 — 点击弹出插件管理菜单。
//! 外观可通过旁边的 style.css 自定义。

use std::ffi::c_void;
use std::path::Path;
use std::sync::Once;
use log::info;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

// ============================================================
// 静态回调函数（由 main.rs 在启动时注入）
// ============================================================

/// 获取当前插件列表
pub static mut FN_PLUGIN_LIST: Option<unsafe fn() -> Vec<crate::plugin_system::PluginInfo>> = None;
/// 切换插件启用状态
pub static mut FN_PLUGIN_TOGGLE: Option<unsafe fn(name: &str, hwnd: HWND) -> crate::plugin_system::ToggleResult> = None;
/// 异步按键处理回调（钩子先拦截，然后通过 PostMessage 调用此函数）
pub static mut FN_PROCESS_KEY: Option<unsafe fn(vkey: u32)> = None;

// ============================================================
// Theme — 视觉参数（可由 style.css 覆盖）
// ============================================================

/// BGR 色值（Win32 COLORREF 格式）
const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((b as u32) << 16 | (g as u32) << 8 | r as u32)
}

/// 所有可定制的视觉参数
#[derive(Clone, Debug)]
pub struct Theme {
    pub bg:         COLORREF,  // --bg-color
    pub text:       COLORREF,  // --text-color
    pub pinyin:     COLORREF,  // --pinyin-color
    pub index:      COLORREF,  // --index-color
    pub hl_bg:      COLORREF,  // --highlight-bg
    pub hl_text:    COLORREF,  // --highlight-text
    pub font_sz:    i32,       // --font-size (px)
    pub pinyin_sz:  i32,       // --pinyin-size (px)
    pub win_radius: i32,       // --corner-radius (px)
    pub pad_h:      i32,       // --padding-h (px)
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg:         rgb(46, 49, 62),     // #2E313E
            text:       rgb(200, 204, 216),  // #C8CCD8
            pinyin:     rgb(169, 177, 214),  // #A9B1D6
            index:      rgb(130, 134, 150),  // #82869C
            hl_bg:      rgb(122, 162, 247),  // #7AA2F7
            hl_text:    rgb(255, 255, 255),  // #FFFFFF
            font_sz:    24,
            pinyin_sz:  22,
            win_radius: 14,
            pad_h:      14,
        }
    }
}

impl Theme {
    /// 从 style.css 加载，失败则静默回退到默认值
    pub fn load() -> Self {
        // 查找顺序：可执行文件同级目录 → 当前工作目录
        let css_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("style.css")))
            .filter(|p| p.exists())
            .or_else(|| {
                let cwd = Path::new("style.css");
                if cwd.exists() { Some(cwd.to_path_buf()) } else { None }
            });

        let Some(path) = css_path else {
            info!("[Theme] 未找到 style.css，使用默认配色");
            return Self::default();
        };

        let Ok(css) = std::fs::read_to_string(&path) else {
            return Self::default();
        };

        let mut theme = Self::default();
        info!("[Theme] 已加载 {:?}", path);

        // 简单的 CSS 变量解析：逐行查找 --var-name: value;
        for line in css.lines() {
            let line = line.trim();
            if !line.starts_with("--") { continue; }

            // 分割 key: value
            let Some(colon) = line.find(':') else { continue; };
            let key = line[..colon].trim();
            // 去掉 value 末尾的 ";" 和注释 "/*...*/"
            let raw_val = line[colon + 1..].trim();
            let val = raw_val
                .split(';').next().unwrap_or("")
                .split("/*").next().unwrap_or("")
                .trim();

            match key {
                "--bg-color"       => { if let Some(c) = parse_hex_color(val) { theme.bg       = c; } }
                "--text-color"     => { if let Some(c) = parse_hex_color(val) { theme.text     = c; } }
                "--pinyin-color"   => { if let Some(c) = parse_hex_color(val) { theme.pinyin   = c; } }
                "--index-color"    => { if let Some(c) = parse_hex_color(val) { theme.index    = c; } }
                "--highlight-bg"   => { if let Some(c) = parse_hex_color(val) { theme.hl_bg    = c; } }
                "--highlight-text" => { if let Some(c) = parse_hex_color(val) { theme.hl_text  = c; } }
                "--font-size"      => { if let Some(n) = parse_px(val)        { theme.font_sz  = n; } }
                "--pinyin-size"    => { if let Some(n) = parse_px(val)        { theme.pinyin_sz= n; } }
                "--corner-radius"  => { if let Some(n) = parse_px(val)        { theme.win_radius=n; } }
                "--padding-h"      => { if let Some(n) = parse_px(val)        { theme.pad_h    = n; } }
                _ => {}
            }
        }

        theme
    }
}

/// 解析 #RRGGBB 格式的颜色值
fn parse_hex_color(s: &str) -> Option<COLORREF> {
    let s = s.trim().strip_prefix('#')?;
    if s.len() != 6 { return None; }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(rgb(r, g, b))
}

/// 解析 "Npx" 格式的像素值
fn parse_px(s: &str) -> Option<i32> {
    s.trim().strip_suffix("px")?.trim().parse().ok()
}

// ============================================================
// 固定排版参数（不暴露给 CSS，太细了没必要）
// ============================================================
const PAD_TOP: i32  = 7;   // 顶部内边距
const PAD_BOT: i32  = 8;   // 底部内边距
const ROW_GAP: i32  = 3;   // 两排间距
const ITEM_GAP: i32 = 22;  // 候选词间距
const HL_PAD_H: i32 = 7;   // 高亮水平内边距
const HL_PAD_V: i32 = 3;   // 高亮垂直内边距
const HL_RADIUS: i32= 7;   // 高亮圆角

const WND_CLASS: PCWSTR = w!("AiPinyinCandidate");
static REGISTER_ONCE: Once = Once::new();

// ============================================================
// WindowState
// ============================================================

struct WindowState {
    raw_input: String,
    candidates: Vec<String>,
    selected: usize,
    theme: Theme,
    font_pinyin: HFONT,
    font_cand: HFONT,
    font_idx: HFONT,
    /// JS 指示灯小字体
    font_small: HFONT,
    /// [JS] 按鈕在客户区的位置
    js_btn_rect: RECT,
    /// [⚙] 设置按钮区域
    settings_btn_rect: RECT,
    /// 当前是否有激活的插件
    plugins_active: bool,
    /// 翻页信息: (current_page, total_pages)  None=不需要显示
    page_info: Option<(usize, usize)>,
}

impl WindowState {
    fn new(theme: Theme) -> Self {
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
                font_pinyin: mk_font(theme.pinyin_sz, FW_NORMAL.0 as i32),
                font_cand:   mk_font(theme.font_sz,   FW_MEDIUM.0 as i32),
                font_idx:    mk_font(theme.font_sz,   FW_NORMAL.0 as i32),
                font_small:  mk_font(12,               FW_NORMAL.0 as i32),
                theme,
                js_btn_rect: RECT::default(),
                settings_btn_rect: RECT::default(),
                plugins_active: false,
                page_info: None,
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

        let theme = Theme::load();
        let state = Box::new(WindowState::new(theme));
        let state_ptr = Box::into_raw(state);

        let hwnd = unsafe {
            let hinstance = GetModuleHandleW(None)?;
            let hinstance_val: HINSTANCE = hinstance.into();
            let ex_style = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;

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

            // DWM 系统级圆角（Win11 DirectComposition 渲染，完全无锯齿）
            // DWMWCP_ROUND = 2；Win10 上该调用无害，静默忽略
            let preference: u32 = 2u32; // DWMWCP_ROUND
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &preference as *const u32 as *const core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );

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
        self.update_candidates_with_page(raw, candidates, None);
    }

    /// 更新候选词 + 翻页信息
    pub fn update_candidates_with_page(&self, raw: &str, candidates: &[&str], page_info: Option<(usize, usize)>) {
        if candidates.is_empty() {
            self.hide();
            return;
        }
        unsafe {
            let state = &mut *self.state;
            state.raw_input = raw.to_string();
            state.candidates = candidates.iter().map(|s| s.to_string()).collect();
            state.selected = 0;
            state.page_info = page_info;
            self.resize_and_redraw(state);
        }
    }

    /// 更新 [JS] 按钮的激活状态（有无运行中的插件）
    pub fn set_plugins_active(&self, active: bool) {
        unsafe {
            let state = &mut *self.state;
            if state.plugins_active != active {
                state.plugins_active = active;
                let _ = InvalidateRect(self.hwnd, None, TRUE);
            }
        }
    }

    /// 获取窗口句柄（用于 PostMessage 异步消息）
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// 在指定屏幕坐标显示并立即绘制（多显示器感知 + 任务栏回避）
    pub fn show(&self, x: i32, y: i32) {
        unsafe {
            let mut wnd_rc = RECT::default();
            let _ = GetWindowRect(self.hwnd, &mut wnd_rc);
            let wnd_w = wnd_rc.right - wnd_rc.left;
            let wnd_h = wnd_rc.bottom - wnd_rc.top;

            // 多显示器感知：获取光标所在显示器的可用工作区（去掉任务栏）
            let caret_pt = POINT { x, y };
            let monitor = MonitorFromPoint(caret_pt, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let work_rc = if GetMonitorInfoW(monitor, &mut mi).as_bool() {
                mi.rcWork
            } else {
                RECT {
                    left: 0, top: 0,
                    right: GetSystemMetrics(SM_CXSCREEN),
                    bottom: GetSystemMetrics(SM_CYSCREEN),
                }
            };

            // 水平：右侧不超出工作区
            let mut fx = x;
            if fx + wnd_w > work_rc.right { fx = work_rc.right - wnd_w; }
            if fx < work_rc.left { fx = work_rc.left; }

            // 垂直：下方优先；超出底部则翻转到光标上方
            let mut fy = y;
            if fy + wnd_h > work_rc.bottom {
                fy = y - wnd_h - 30; // 光标上方
            }
            if fy < work_rc.top { fy = work_rc.top; }

            let _ = SetWindowPos(
                self.hwnd, HWND_TOPMOST, fx, fy, 0, 0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = UpdateWindow(self.hwnd);
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

    // ── 内部：调整尺寸 + 立即重绘 ──
    unsafe fn resize_and_redraw(&self, state: &WindowState) {
        let hdc = GetDC(self.hwnd);
        let (w, h) = calc_size(hdc, state);
        ReleaseDC(self.hwnd, hdc);

        if w <= 0 || h <= 0 { return; }

        let _ = SetWindowPos(
            self.hwnd, None, 0, 0, w, h,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );

        // DWM 圆角已在窗口创建时设置，不使用 CreateRoundRectRgn（其边缘有锯齿）
        // RedrawWindow 立即同步绘制，不依赖消息队列
        let _ = RedrawWindow(
            self.hwnd, None, None,
            RDW_INVALIDATE | RDW_UPDATENOW | RDW_ERASE,
        );
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
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() { paint(hdc, hwnd, &mut *ptr); }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONDOWN => {
            // 点击客户区 (JS 按钮或 ⚙ 按钮)
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                let state = &*ptr;
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let pt = POINT { x, y };
                if PtInRect(&state.settings_btn_rect, pt).as_bool() {
                    crate::settings::open_settings();
                } else if PtInRect(&state.js_btn_rect, pt).as_bool() {
                    show_plugin_menu(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_NCHITTEST => {
            // JS 按钮区域 → HTCLIENT (保留点击), 其余 → HTCAPTION (可拖动)
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                let state = &*ptr;
                // 将屏幕坐标转为客户区坐标
                let mut pt = POINT {
                    x: (lparam.0 & 0xFFFF) as i16 as i32,
                    y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
                };
                let _ = ScreenToClient(hwnd, &mut pt);
                if PtInRect(&state.settings_btn_rect, pt).as_bool()
                    || PtInRect(&state.js_btn_rect, pt).as_bool() {
                    return LRESULT(1); // HTCLIENT
                }
            }
            LRESULT(2) // HTCAPTION → 可拖动
        }
        WM_KEYDOWN if wparam.0 == 0x1B => { // ESC
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        // 钩子异步按键处理：先拦截再通过 PostMessage 到这里处理
        x if x == crate::WM_IME_KEYDOWN => {
            let vkey = wparam.0 as u32;
            if let Some(f) = FN_PROCESS_KEY {
                f(vkey);
            }
            LRESULT(0)
        }
        // AI 后台线程完成推理, 在主线程安全更新 UI
        x if x == crate::WM_AI_RESULT => {
            if let Some((gen, raw, merged)) = crate::AI_RESULT.take() {
                let state_ptr = crate::GLOBAL_STATE;
                if !state_ptr.is_null() {
                    let state = &mut *state_ptr;
                    if state.ai_generation == gen {
                        if !merged.is_empty() {
                            state.all_candidates = merged;
                            state.page_offset = 0;
                            crate::show_current_page(state, &raw);
                            eprintln!("[AI] 异步更新候选: {} 条", state.all_candidates.len());
                            // 联想模式：结果出来后才定位并显示窗口（此时尺寸正确）
                            if state.prediction_mode {
                                let pt = crate::get_caret_screen_pos();
                                state.cand_win.show(pt.x, pt.y + 4);
                            }
                        }
                    }
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 弹出插件管理菜单
unsafe fn show_plugin_menu(hwnd: HWND) {
    let list = match FN_PLUGIN_LIST {
        Some(f) => f(),
        None => return,
    };
    if list.is_empty() {
        MessageBoxW(hwnd,
            w!("plugins/ 目录下暂无插件，请在 plugins/ 目录中放置 .js 文件。"),
            w!("AiPinyin 插件"),
            MB_OK | MB_ICONINFORMATION);
        return;
    }

    let menu = match CreatePopupMenu() {
        Ok(m) => m,
        Err(_) => return,
    };

    // 瞎标当前位置展示菜单
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);

    // 添加标题行（灰色不可点）
    let title_w: Vec<u16> = format!("插件管理  (最多 {} 个同时激活)",
        crate::plugin_system::MAX_ACTIVE)
        .encode_utf16().chain(std::iter::once(0)).collect();
    let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED,
        0, PCWSTR(title_w.as_ptr()));
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());

    // 每个插件一行
    for (i, info) in list.iter().enumerate() {
        let label = format!("{} {}{}",
            if info.enabled { "●" } else { "○" },
            info.name,
            if info.authorized { "" } else { "  [未授权]" });
        let label_w: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
        let flags = MF_STRING
            | if info.enabled { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, flags, i + 1, PCWSTR(label_w.as_ptr()));
    }

    // 显示菜单并等待选择
    let id = TrackPopupMenu(
        menu,
        TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD | TPM_NONOTIFY,
        pt.x, pt.y, 0, hwnd, None,
    );
    let _ = DestroyMenu(menu);

    // 处理选择
    if id.0 > 0 {
        let idx = (id.0 as usize) - 1;
        if let Some(info) = list.get(idx) {
            if let Some(toggle) = FN_PLUGIN_TOGGLE {
                toggle(&info.name, hwnd);
                // 重绘更新按鈕状态
                let _ = InvalidateRect(hwnd, None, TRUE);
                let _ = UpdateWindow(hwnd);
            }
        }
    }
}

// ============================================================
// 绘制：双排布局
// ============================================================

unsafe fn paint(hdc: HDC, hwnd: HWND, state: &mut WindowState) {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);

    // ── 背景 ──
    let bg_brush = CreateSolidBrush(state.theme.bg);
    FillRect(hdc, &rc, bg_brush);
    let _ = DeleteObject(bg_brush);

    // ── 右上角按钮: ⚙ 设置(主) + JS 指示灯(小) ──
    {
        let btn_pad = 3i32;

        // 计算拼音行的垂直中心, 用于按钮居中
        SelectObject(hdc, state.font_pinyin);
        let sample_py: Vec<u16> = "py".encode_utf16().collect();
        let mut py_sz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &sample_py, &mut py_sz);
        let py_mid = PAD_TOP + py_sz.cy / 2; // 拼音行垂直中心

        // ⚙ 设置按钮 (最右, 用序号字体)
        SelectObject(hdc, state.font_pinyin);
        let gear: Vec<u16> = "⚙".encode_utf16().collect();
        let mut gsz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &gear, &mut gsz);
        let gx = rc.right - gsz.cx - btn_pad * 2 - state.theme.pad_h;
        let gy = py_mid - gsz.cy / 2; // 垂直居中
        SetTextColor(hdc, state.theme.index);
        SetBkMode(hdc, TRANSPARENT);
        let _ = TextOutW(hdc, gx, gy, &gear);
        state.settings_btn_rect = RECT {
            left: gx - btn_pad,  top: gy - btn_pad,
            right: gx + gsz.cx + btn_pad, bottom: gy + gsz.cy + btn_pad,
        };

        // JS 指示灯 (小字体, 在 ⚙ 左边)
        SelectObject(hdc, state.font_small);
        let js_label: Vec<u16> = "JS".encode_utf16().collect();
        let mut jsz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &js_label, &mut jsz);
        let jx = gx - jsz.cx - btn_pad * 3;
        let jy = py_mid - jsz.cy / 2; // 垂直居中

        state.js_btn_rect = RECT {
            left: jx - btn_pad, top: jy - btn_pad,
            right: jx + jsz.cx + btn_pad, bottom: jy + jsz.cy + btn_pad,
        };

        if state.plugins_active {
            let b = CreateSolidBrush(state.theme.hl_bg);
            let old = SelectObject(hdc, b);
            let p = SelectObject(hdc, GetStockObject(NULL_PEN));
            let _ = RoundRect(hdc,
                state.js_btn_rect.left, state.js_btn_rect.top,
                state.js_btn_rect.right, state.js_btn_rect.bottom, 4, 4);
            SelectObject(hdc, p);
            SelectObject(hdc, old);
            let _ = DeleteObject(b);
            SetTextColor(hdc, state.theme.hl_text);
        } else {
            SetTextColor(hdc, state.theme.index);
        }
        let _ = TextOutW(hdc, jx, jy, &js_label);
    }

    if state.candidates.is_empty() { return; }

    SetBkMode(hdc, TRANSPARENT);

    // ── 上排：拼音原文 ──
    if !state.raw_input.is_empty() {
        SelectObject(hdc, state.font_pinyin);
        SetTextColor(hdc, state.theme.pinyin);
        let w: Vec<u16> = state.raw_input.encode_utf16().collect();
        let _ = TextOutW(hdc, state.theme.pad_h, PAD_TOP, &w);
    }

    // ── 下排：候选词 ──
    // 计算上排高度，用于定位下排
    let pinyin_h = pinyin_row_height(hdc, state);
    let y_cand = PAD_TOP + pinyin_h + ROW_GAP;
    let y_mid  = y_cand + (cand_row_height(hdc, state)) / 2;

    let mut x = state.theme.pad_h;

    for (i, cand) in state.candidates.iter().enumerate() {
        let is_sel = i == state.selected;

        // 序号
        SelectObject(hdc, state.font_idx);
        SetTextColor(hdc, if is_sel { state.theme.hl_text } else { state.theme.index });
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
            let hl_brush = CreateSolidBrush(state.theme.hl_bg);
            let ob = SelectObject(hdc, hl_brush);
            let op = SelectObject(hdc, GetStockObject(NULL_PEN));
            let _ = RoundRect(hdc, hl_rc.left, hl_rc.top, hl_rc.right, hl_rc.bottom,
                              HL_RADIUS, HL_RADIUS);
            SelectObject(hdc, op);
            SelectObject(hdc, ob);
            let _ = DeleteObject(hl_brush);
        }

        // 候选字
        SetTextColor(hdc, if is_sel { state.theme.hl_text } else { state.theme.text });
        let _ = TextOutW(hdc, x, text_y, &cw);
        x += csz.cx + ITEM_GAP;
    }

    // ── 翻页箭头 (在候选词最后) ──
    if let Some((page, total)) = state.page_info {
        SelectObject(hdc, state.font_idx);
        let arrows = format!("{}/{}", page, total);
        let aw: Vec<u16> = arrows.encode_utf16().collect();
        let mut asz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &aw, &mut asz);
        SetTextColor(hdc, state.theme.index);
        let _ = TextOutW(hdc, x + 4, y_mid - asz.cy / 2, &aw);
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
    let mut total_w = state.theme.pad_h * 2;
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
        total_w = total_w.max(psz.cx + state.theme.pad_h * 2);
    }

    // 高度：上排 + 间隔 + 下排 + 上下内边距
    let ph = pinyin_row_height(hdc, state);
    let ch = cand_row_height(hdc, state);
    let h = PAD_TOP + ph + if ph > 0 { ROW_GAP } else { 0 } + ch + PAD_BOT;

    (total_w, h)
}
