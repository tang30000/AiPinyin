//! # AiPinyin — AI 驱动的轻量级本地拼音输入法
//!
//! 架构：WH_KEYBOARD_LL 全局键盘钩子 + 多策略光标定位

mod guardian;
pub mod key_event;
pub mod pinyin;
pub mod ui;

use std::cmp::min;
use anyhow::Result;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use crate::key_event::{InputState, handle_key_down};

pub const CLSID_AIPINYIN: GUID = GUID::from_u128(0xe0e55f04_f427_45f7_86a1_ac150445bcde);

// ============================================================
// 全局状态
// ============================================================

struct ImeState {
    input: InputState,
    cand_win: ui::CandidateWindow,
}

static mut GLOBAL_STATE: *mut ImeState = std::ptr::null_mut();

// ============================================================
// 主入口
// ============================================================

fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn") // 生产级：减少日志噪音
    ).init();

    println!();
    println!("  ╔══════════════════════════════════════════╗");
    println!("  ║    AiPinyin 爱拼音 v{}          ║", env!("CARGO_PKG_VERSION"));
    println!("  ║    AI驱动 · 向量引擎 · 本地推理          ║");
    println!("  ╚══════════════════════════════════════════╝");
    println!();
    println!("  在任意窗口直接打拼音即可！");
    println!("  A-Z: 输入 | 空格/数字: 上屏 | 退格: 删除 | ESC: 取消");
    println!();

    let _guardian = guardian::start_guardian(guardian::GuardianConfig::default());

    let cand_win = ui::CandidateWindow::new()?;
    let state = Box::new(ImeState {
        input: InputState::new(),
        cand_win,
    });

    unsafe {
        GLOBAL_STATE = Box::into_raw(state);

        let hinstance = GetModuleHandleW(None)?;
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_hook),
            hinstance,
            0,
        )?;
        println!("  ✅ 全局钩子已安装，请切换到其他窗口打字...");

        // 消息循环（不创建任何窗口，只驱动钩子和候选窗口）
        ui::run_message_loop();

        let _ = UnhookWindowsHookEx(hook);
        let _ = Box::from_raw(GLOBAL_STATE);
        GLOBAL_STATE = std::ptr::null_mut();
    }

    Ok(())
}

// ============================================================
// 全局低阶键盘钩子
// ============================================================

unsafe extern "system" fn low_level_keyboard_hook(
    code: i32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    if code == 0
        && (wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize)
    {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);

        if !GLOBAL_STATE.is_null() {
            let state = &mut *GLOBAL_STATE;
            let result = handle_key_down(&mut state.input, info.vkCode);

            // 有文本需要上屏（汉字候选 或 原始字母）
            if let Some(text) = result.committed {
                state.cand_win.hide();
                // 先让钩子返回，再注入文本（避免递归）
                // 用 PostMessage 延迟注入
                send_unicode_text(&text);
            }

            if result.need_refresh {
                refresh_candidates(state);
            }

            if result.eaten {
                return LRESULT(1); // 吞掉原始按键
            }
        }
    }

    CallNextHookEx(HHOOK(std::ptr::null_mut()), code, wparam, lparam)
}

/// 向当前焦点应用注入 Unicode 文本
///
/// 每个字符用 SendInput + KEYEVENTF_UNICODE 发送，
/// 支持任意 Unicode（中文、符号等）。
unsafe fn send_unicode_text(text: &str) {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;

    let inputs: Vec<INPUT> = text
        .encode_utf16()
        .flat_map(|wchar| {
            // 每个字符发一个 keydown + keyup
            [
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(0),
                            wScan: wchar,
                            dwFlags: KEYEVENTF_UNICODE,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(0),
                            wScan: wchar,
                            dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                },
            ]
        })
        .collect();

    if !inputs.is_empty() {
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        eprintln!("[IME] 注入 {} 个字符，sent={}", text.chars().count(), sent);
    }
}


// ============================================================
// 候选词刷新 + 多策略光标定位
// ============================================================

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

    let raw = state.input.engine.raw_input().to_string();
    eprintln!("[IME] raw={:?}, cands={}", raw, refs.len());
    state.cand_win.update_candidates(&raw, &refs[..count]);

    let pt = get_caret_screen_pos();
    eprintln!("[IME] show at ({}, {})", pt.x, pt.y + 4);
    state.cand_win.show(pt.x, pt.y + 4);
}

/// 多策略获取光标屏幕坐标
///
/// 策略1: GetGUIThreadInfo — 适用于普通权限应用 (记事本、浏览器等)
/// 策略2: GetCaretPos + ClientToScreen — 适用于同进程窗口
/// 策略3: 鼠标位置 — 通用回退（鼠标通常在正在输入的文字旁边）
unsafe fn get_caret_screen_pos() -> POINT {
    let fg = GetForegroundWindow();

    // ── 策略1: GetGUIThreadInfo ──
    if !fg.is_invalid() {
        let thread_id = GetWindowThreadProcessId(fg, None);
        let mut gi = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread_id, &mut gi).is_ok() && !gi.hwndCaret.is_invalid() {
            let mut pt = POINT {
                x: gi.rcCaret.left,
                y: gi.rcCaret.bottom,
            };
            let _ = ClientToScreen(gi.hwndCaret, &mut pt);
            // 合理性检查：坐标要在屏幕范围内
            if pt.x > 0 && pt.y > 0 {
                return pt;
            }
        }
    }

    // ── 策略2: GetCaretPos (同线程)──
    let mut pt = POINT::default();
    if GetCaretPos(&mut pt).is_ok() && !fg.is_invalid() {
        let mut spt = pt;
        if ClientToScreen(fg, &mut spt).as_bool() && spt.x > 0 && spt.y > 0 {
            return POINT { x: spt.x, y: spt.y + 20 };
        }
    }

    // ── 策略3: 鼠标光标位置（偏移下方）──
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    POINT { x: pt.x, y: pt.y + 24 }
}
