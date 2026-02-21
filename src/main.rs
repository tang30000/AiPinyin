//! # AiPinyin — AI 驱动的轻量级本地拼音输入法
//!
//! 演示模式：测试输入窗口 + 候选词窗口联动。
//! 在测试窗口中打字，候选词实时刷新。

mod guardian;
pub mod key_event;
pub mod pinyin;
pub mod ui;

use std::cmp::min;
use std::ffi::c_void;
use anyhow::Result;
use log::info;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::TextServices::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use crate::key_event::{InputState, handle_key_down};

// ============================================================
// TSF 文本服务骨架
// ============================================================

pub const CLSID_AIPINYIN: GUID = GUID::from_u128(0xe0e55f04_f427_45f7_86a1_ac150445bcde);

#[derive(Debug)]
pub struct AiPinyinTextService {
    thread_mgr: Option<ITfThreadMgr>,
    client_id: u32,
}

impl AiPinyinTextService {
    pub fn new() -> Self { Self { thread_mgr: None, client_id: 0 } }
}

// ============================================================
// 演示状态：输入窗口 + 候选窗口
// ============================================================

/// 演示模式的共享状态
struct DemoState {
    input: InputState,
    cand_win: ui::CandidateWindow,
}

const TEST_CLASS: PCWSTR = w!("AiPinyinTestInput");

// 测试窗口的设计参数
const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((b as u32) << 16 | (g as u32) << 8 | r as u32)
}
const TEST_BG: COLORREF = rgb(24, 24, 37);
const TEST_TEXT: COLORREF = rgb(205, 214, 244);
const TEST_PINYIN: COLORREF = rgb(122, 162, 247);
const TEST_HINT: COLORREF = rgb(88, 91, 112);

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

    // 启动 Guardian
    let _guardian = guardian::start_guardian(guardian::GuardianConfig::default());
    info!("✅ Guardian 已启动");

    // 创建候选词窗口
    let cand_win = ui::CandidateWindow::new()?;

    // 创建演示状态
    let demo = Box::new(DemoState {
        input: InputState::new(),
        cand_win,
    });
    let demo_ptr = Box::into_raw(demo);

    // 注册并创建测试输入窗口
    let hwnd = create_test_window(demo_ptr)?;
    info!("✅ 测试输入窗口已创建 — 直接打字试试！");
    info!("   A-Z: 输入拼音 | 空格/数字: 上屏 | 退格: 删除 | ESC: 取消");

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }

    // 消息循环
    ui::run_message_loop();

    // 清理
    unsafe { let _ = Box::from_raw(demo_ptr); }
    info!("AiPinyin 已退出");
    Ok(())
}

// ============================================================
// 测试输入窗口
// ============================================================

fn create_test_window(state: *mut DemoState) -> Result<HWND> {
    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let hinstance_val: HINSTANCE = hinstance.into();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(test_wnd_proc),
            hInstance: hinstance_val,
            lpszClassName: TEST_CLASS,
            hCursor: LoadCursorW(None, IDC_IBEAM).ok().unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            TEST_CLASS,
            w!("AiPinyin 测试输入"),
            WS_OVERLAPPEDWINDOW,
            200, 300, 700, 60,
            None, None,
            hinstance_val,
            Some(state as *const c_void),
        )?;

        Ok(hwnd)
    }
}

unsafe extern "system" fn test_wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            LRESULT(0)
        }

        WM_KEYDOWN => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DemoState;
            if ptr.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
            let demo = &mut *ptr;

            let vkey = wparam.0 as u32;
            let result = handle_key_down(&mut demo.input, vkey);

            if result.need_refresh {
                if demo.input.engine.is_empty() {
                    demo.cand_win.hide();
                } else {
                    // 查询拼音引擎获取候选词
                    let cands = demo.input.engine.get_candidates();
                    let refs: Vec<&str> = cands.iter().map(|s| s.as_str()).collect();
                    let count = min(9, refs.len());
                    // 一站式更新：传入候选 → 自动定位到光标 → 显示
                    demo.cand_win.update_candidates(&refs[..count]);
                }
                // 重绘输入窗口
                let _ = InvalidateRect(hwnd, None, TRUE);
            }

            if result.eaten { LRESULT(0) } else { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        WM_PAINT => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const DemoState;
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            // 深色背景
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let bg = CreateSolidBrush(TEST_BG);
            FillRect(hdc, &rc, bg);
            let _ = DeleteObject(bg);

            if !ptr.is_null() {
                let demo = &*ptr;
                let font = CreateFontW(
                    20, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0,
                    DEFAULT_CHARSET.0 as u32, OUT_TT_PRECIS.0 as u32,
                    CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32,
                    DEFAULT_PITCH.0 as u32, w!("微软雅黑"),
                );
                SelectObject(hdc, font);
                SetBkMode(hdc, TRANSPARENT);

                let mut x = 10;
                let y = (rc.bottom - 20) / 2;

                // 绘制已提交文字
                if !demo.input.committed.is_empty() {
                    SetTextColor(hdc, TEST_TEXT);
                    let w: Vec<u16> = demo.input.committed.encode_utf16().collect();
                    let mut sz = SIZE::default();
                    let _ = GetTextExtentPoint32W(hdc, &w, &mut sz);
                    let _ = TextOutW(hdc, x, y, &w);
                    x += sz.cx;
                }

                // 绘制当前拼音输入
                let raw = demo.input.engine.raw_input();
                if !raw.is_empty() {
                    SetTextColor(hdc, TEST_PINYIN);
                    let w: Vec<u16> = raw.encode_utf16().collect();
                    let _ = TextOutW(hdc, x, y, &w);
                } else if demo.input.committed.is_empty() {
                    // 占位提示
                    SetTextColor(hdc, TEST_HINT);
                    let hint = "请直接打字试试... (A-Z 输入拼音)";
                    let w: Vec<u16> = hint.encode_utf16().collect();
                    let _ = TextOutW(hdc, x, y, &w);
                }

                let _ = DeleteObject(font);
            }

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
