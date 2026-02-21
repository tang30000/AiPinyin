//! # AiPinyin — AI 驱动的轻量级本地拼音输入法
//!
//! 架构：WH_KEYBOARD_LL 全局键盘钩子 + GetGUIThreadInfo 光标定位

mod guardian;
pub mod key_event;
pub mod pinyin;
pub mod ui;

use std::cmp::min;
use anyhow::Result;
use log::info;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use crate::key_event::{InputState, handle_key_down};

pub const CLSID_AIPINYIN: GUID = GUID::from_u128(0xe0e55f04_f427_45f7_86a1_ac150445bcde);

// ============================================================
// 全局状态（钩子回调是裸函数，必须用全局）
// ============================================================

struct ImeState {
    input: InputState,
    cand_win: ui::CandidateWindow,
    status_hwnd: HWND,  // 状态条窗口（显示拼音和已提交文字）
}

static mut GLOBAL_STATE: *mut ImeState = std::ptr::null_mut();

// ============================================================
// 主入口
// ============================================================

fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    println!();
    println!("  ╔══════════════════════════════════════════╗");
    println!("  ║    AiPinyin 爱拼音 v{}          ║", env!("CARGO_PKG_VERSION"));
    println!("  ║    AI驱动 · 向量引擎 · 本地推理          ║");
    println!("  ╚══════════════════════════════════════════╝");
    println!();

    let _guardian = guardian::start_guardian(guardian::GuardianConfig::default());
    info!("✅ Guardian 已启动");

    let cand_win = ui::CandidateWindow::new()?;
    let status_hwnd = create_status_bar()?;

    let state = Box::new(ImeState {
        input: InputState::new(),
        cand_win,
        status_hwnd,
    });

    unsafe {
        GLOBAL_STATE = Box::into_raw(state);

        // 安装全局低阶键盘钩子
        let hinstance = GetModuleHandleW(None)?;
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_hook),
            hinstance,
            0, // 0 = 所有线程（全局）
        )?;
        info!("✅ 全局键盘钩子已安装 — 在任意窗口打字即可！");
        info!("   A-Z: 输入拼音 | 空格/数字: 上屏 | 退格: 删除 | ESC: 取消");

        let _ = ShowWindow(status_hwnd, SW_SHOW);
        let _ = SetForegroundWindow(status_hwnd);

        // 消息循环
        ui::run_message_loop();

        // 卸载钩子并清理
        let _ = UnhookWindowsHookEx(hook);
        let _ = Box::from_raw(GLOBAL_STATE);
        GLOBAL_STATE = std::ptr::null_mut();
    }

    info!("AiPinyin 已退出");
    Ok(())
}

// ============================================================
// 全局低阶键盘钩子回调
// ============================================================

unsafe extern "system" fn low_level_keyboard_hook(
    code: i32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    // HC_ACTION = 0，只处理实际按键消息
    if code == 0 && (wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize) {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vkey = info.vkCode;

        if !GLOBAL_STATE.is_null() {
            let state = &mut *GLOBAL_STATE;
            let result = handle_key_down(&mut state.input, vkey);

            if result.need_refresh {
                // 刷新候选窗口
                refresh_candidates(state);
                // 重绘状态条
                let _ = InvalidateRect(state.status_hwnd, None, TRUE);
            }

            if result.eaten {
                return LRESULT(1); // 吞掉按键，不传给目标应用
            }
        }
    }

    CallNextHookEx(HHOOK(std::ptr::null_mut()), code, wparam, lparam)
}

/// 刷新候选词列表并定位到真实光标位置
unsafe fn refresh_candidates(state: &mut ImeState) {
    if state.input.engine.is_empty() {
        state.cand_win.hide();
        return;
    }

    let cands = state.input.engine.get_candidates();
    let refs: Vec<&str> = cands.iter().map(|s| s.as_str()).collect();
    let count = min(9, refs.len());
    if count == 0 {
        state.cand_win.hide();
        return;
    }

    state.cand_win.draw_candidates(&refs[..count]);

    // 用 GetGUIThreadInfo 获取当前焦点线程的真实光标位置
    let pt = get_real_caret_pos();
    state.cand_win.show(pt.x, pt.y + 4);
}

/// 通过 GetGUIThreadInfo 获取当前焦点应用的文本光标屏幕坐标
unsafe fn get_real_caret_pos() -> POINT {
    let fg = GetForegroundWindow();
    if fg.is_invalid() {
        return POINT { x: 100, y: 200 };
    }

    let thread_id = GetWindowThreadProcessId(fg, None);
    let mut gi = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };

    if GetGUIThreadInfo(thread_id, &mut gi).is_ok()
        && !gi.hwndCaret.is_invalid()
    {
        // rcCaret 是光标在 hwndCaret 客户区的矩形
        let mut pt = POINT {
            x: gi.rcCaret.left,
            y: gi.rcCaret.bottom, // 用底边，候选窗口显示在文字下面
        };
        let _ = ClientToScreen(gi.hwndCaret, &mut pt);
        return pt;
    }

    // 回退：用鼠标位置
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    pt
}

// ============================================================
// 状态条窗口 — 显示拼音缓冲和已提交文字
// ============================================================

const STATUS_CLASS: PCWSTR = w!("AiPinyinStatus");

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((b as u32) << 16 | (g as u32) << 8 | r as u32)
}
const S_BG:     COLORREF = rgb(30, 30, 46);
const S_TEXT:   COLORREF = rgb(205, 214, 244);
const S_PINYIN: COLORREF = rgb(122, 162, 247);
const S_HINT:   COLORREF = rgb(88, 91, 112);

fn create_status_bar() -> Result<HWND> {
    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(status_wnd_proc),
            hInstance: hinstance,
            lpszClassName: STATUS_CLASS,
            hCursor: LoadCursorW(None, IDC_ARROW).ok().unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            STATUS_CLASS,
            w!("AiPinyin 爱拼音"),
            WS_POPUP | WS_BORDER,
            50, 50, 520, 44,
            None, None,
            hinstance,
            None,
        )?;

        Ok(hwnd)
    }
}

unsafe extern "system" fn status_wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);

            let bg = CreateSolidBrush(S_BG);
            FillRect(hdc, &rc, bg);
            let _ = DeleteObject(bg);

            SetBkMode(hdc, TRANSPARENT);

            let font = CreateFontW(
                18, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0,
                DEFAULT_CHARSET.0 as u32, OUT_TT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32,
                DEFAULT_PITCH.0 as u32, w!("微软雅黑"),
            );
            SelectObject(hdc, font);

            let y = (rc.bottom - 18) / 2;
            let mut x = 10i32;

            if !GLOBAL_STATE.is_null() {
                let state = &*GLOBAL_STATE;

                // 已提交文字
                if !state.input.committed.is_empty() {
                    SetTextColor(hdc, S_TEXT);
                    let w: Vec<u16> = state.input.committed.encode_utf16().collect();
                    let mut sz = SIZE::default();
                    let _ = GetTextExtentPoint32W(hdc, &w, &mut sz);
                    let _ = TextOutW(hdc, x, y, &w);
                    x += sz.cx + 4;
                }

                // 当前拼音输入（蓝色）
                let raw = state.input.engine.raw_input();
                if !raw.is_empty() {
                    SetTextColor(hdc, S_PINYIN);
                    let w: Vec<u16> = raw.encode_utf16().collect();
                    let _ = TextOutW(hdc, x, y, &w);
                } else if state.input.committed.is_empty() {
                    SetTextColor(hdc, S_HINT);
                    let hint: Vec<u16> = "在任意窗口打字即可 (A-Z 输入拼音)".encode_utf16().collect();
                    let _ = TextOutW(hdc, x, y, &hint);
                }
            }

            let _ = DeleteObject(font);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
