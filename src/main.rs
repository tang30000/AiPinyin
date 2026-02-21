//! # AiPinyin — AI 驱动的轻量级本地拼音输入法
//!
//! 架构：WH_KEYBOARD_LL 全局键盘钩子 + 多策略光标定位

mod guardian;
pub mod ai_engine;
pub mod config;
pub mod key_event;
pub mod pinyin;
pub mod plugin_system;
pub mod ui;


use anyhow::Result;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use crate::key_event::{InputState, CommitAction, handle_key_down};

/// 自定义消息: 钩子先拦截按键，然后通过此消息异步处理
const WM_IME_KEYDOWN: u32 = WM_APP + 1;

pub const CLSID_AIPINYIN: GUID = GUID::from_u128(0xe0e55f04_f427_45f7_86a1_ac150445bcde);

// ============================================================
// 全局状态
// ============================================================

struct ImeState {
    input: InputState,
    cand_win: ui::CandidateWindow,
    plugins: plugin_system::PluginSystem,
    ai: ai_engine::AIPredictor,
    history: ai_engine::HistoryBuffer,
    cfg: config::Config,
    /// 候选窗口当前显示的候选词（经过插件+AI处理后）
    current_candidates: Vec<String>,
    chinese_mode: bool,
    shift_down: bool,
    shift_modified: bool,
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

    // 加载 JS 插件（exe 旁的 plugins/ 目录）
    let mut plugins = plugin_system::PluginSystem::new()?;
    let plugins_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("plugins")))
        .unwrap_or_else(|| std::path::PathBuf::from("plugins"));
    plugins.load_dir(&plugins_dir);

    // 加载配置
    let cfg = config::Config::load();

    // 初始化 AI 推理引擎
    let mut ai = ai_engine::AIPredictor::new();
    // 应用配置: 引擎模式
    ai.ai_first = cfg.engine.mode == config::EngineMode::Ai;
    let history = ai_engine::HistoryBuffer::new(10);

    let cand_win = ui::CandidateWindow::new()?;
    let state = Box::new(ImeState {
        input: InputState::new(),
        cand_win,
        plugins,
        ai,
        history,
        cfg,
        current_candidates: Vec::new(),
        chinese_mode: true,
        shift_down: false,
        shift_modified: false,
    });

    unsafe {
        GLOBAL_STATE = Box::into_raw(state);

        // 注册 UI ↔ 插件系统 的回调
        ui::FN_PLUGIN_LIST = Some(cb_plugin_list);
        ui::FN_PLUGIN_TOGGLE = Some(cb_plugin_toggle);
        // 注册异步按键处理回调
        ui::FN_PROCESS_KEY = Some(cb_process_key);

        // 初始化 [JS] 按钮状态
        let s = &*GLOBAL_STATE;
        s.cand_win.set_plugins_active(s.plugins.has_active());

        let hinstance = GetModuleHandleW(None)?;
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_hook),
            hinstance,
            0,
        )?;
        println!("  ✅ 全局钩子已安装，请切换到其他窗口打字...");
        println!("  【Shift】切换中/英文模式");

        // 消息循环（不创建任何窗口，只驱动钩子和候选窗口）
        ui::run_message_loop();

        let _ = UnhookWindowsHookEx(hook);
        let _ = Box::from_raw(GLOBAL_STATE);
        GLOBAL_STATE = std::ptr::null_mut();
    }

    Ok(())
}

// ============================================================
// 插件 UI 回调（由 ui::show_plugin_menu 调用）
// ============================================================

unsafe fn cb_plugin_list() -> Vec<plugin_system::PluginInfo> {
    if GLOBAL_STATE.is_null() { return vec![]; }
    (*GLOBAL_STATE).plugins.plugin_list()
}

unsafe fn cb_plugin_toggle(name: &str, hwnd: HWND) -> plugin_system::ToggleResult {
    if GLOBAL_STATE.is_null() { return plugin_system::ToggleResult::Denied; }
    let state = &mut *GLOBAL_STATE;
    let result = state.plugins.toggle(name, hwnd);
    state.cand_win.set_plugins_active(state.plugins.has_active());
    result
}

// ============================================================
// 异步按键处理回调（由 wnd_proc 收到 WM_IME_KEYDOWN 后调用）
// ============================================================

unsafe fn cb_process_key(vkey: u32) {
    if GLOBAL_STATE.is_null() { return; }
    let state = &mut *GLOBAL_STATE;

    // 调用原有的按键处理逻辑
    let result = handle_key_down(&mut state.input, vkey);

    match result.commit {
        Some(CommitAction::Index(idx)) => {
            let text = state.current_candidates.get(idx).cloned()
                .unwrap_or_default();
            if !text.is_empty() {
                state.cand_win.hide();
                state.current_candidates.clear();
                state.history.push(&text);  // 记录上屏历史
                eprintln!("[IME] \u{2191} \u{4e0a}\u{5c4f} {:?}  (sent={})", text,
                    send_unicode_text(&text));
            }
        }
        Some(CommitAction::Text(text)) => {
            state.cand_win.hide();
            state.current_candidates.clear();
            state.history.push(&text);  // 记录上屏历史
            eprintln!("[IME] \u{2191} \u{4e0a}\u{5c4f} {:?}  (sent={})", text,
                send_unicode_text(&text));
        }
        None => {}
    }

    if result.need_refresh {
        refresh_candidates(state);
    }
}

// ============================================================
// 全局低阶键盘钩子
// ============================================================

unsafe extern "system" fn low_level_keyboard_hook(
    code: i32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    if code != 0 || GLOBAL_STATE.is_null() {
        return CallNextHookEx(HHOOK(std::ptr::null_mut()), code, wparam, lparam);
    }

    let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    let vkey = info.vkCode;
    let state = &mut *GLOBAL_STATE;

    // Shift 键（左/右/通用）
    let is_shift = vkey == 0x10 || vkey == 0xA0 || vkey == 0xA1;

    match wparam.0 as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if is_shift {
                // 记录 Shift 按下，等待判断是否单独抬起
                state.shift_down = true;
                state.shift_modified = false;
                // Shift 本身不吃掉
                return CallNextHookEx(HHOOK(std::ptr::null_mut()), code, wparam, lparam);
            }

            // 有其他键与 Shift 同时按 → 不是单独 Shift
            if state.shift_down {
                state.shift_modified = true;
            }

            // 英文直通模式：所有键直接放行
            if !state.chinese_mode {
                return CallNextHookEx(HHOOK(std::ptr::null_mut()), code, wparam, lparam);
            }

            // 中文模式：先判断是否要拦截，立即返回，再异步处理
            let should_eat = match vkey {
                0x41..=0x5A => true,  // A-Z
                0x08 => !state.input.engine.is_empty(), // Backspace
                0x20 => !state.input.engine.is_empty(), // Space
                0x31..=0x39 => !state.input.engine.is_empty(), // 1-9
                0x1B => !state.input.engine.is_empty(), // Escape
                0x0D => !state.input.engine.is_empty(), // Enter
                _ => false,
            };

            if should_eat {
                // 立即拦截，通过 PostMessage 异步处理（避免钩子超时）
                let _ = PostMessageW(
                    state.cand_win.hwnd(),
                    WM_IME_KEYDOWN,
                    WPARAM(vkey as usize),
                    LPARAM(0),
                );
                return LRESULT(1);
            }
        }

        WM_KEYUP | WM_SYSKEYUP => {
            if is_shift && state.shift_down {
                state.shift_down = false;
                if !state.shift_modified {
                    // 单独 Shift → 切换中英文模式
                    toggle_mode(state);
                }
                state.shift_modified = false;
            }
        }

        _ => {}
    }

    CallNextHookEx(HHOOK(std::ptr::null_mut()), code, wparam, lparam)
}

/// 切换中英文模式
unsafe fn toggle_mode(state: &mut ImeState) {
    state.chinese_mode = !state.chinese_mode;

    if !state.chinese_mode {
        // 切换到英文：若有未提交的拼音，直接以字母形式输出
        if !state.input.engine.is_empty() {
            let raw = state.input.engine.raw_input().to_string();
            state.input.engine.clear();
            send_unicode_text(&raw);
        }
        state.cand_win.hide();
        eprintln!("[IME] ⌨  EN → 英文直通（按 Shift 切回中文）");
    } else {
        eprintln!("[IME] 🀄 CN → 中文拦截（按 Shift 切回英文）");
    }
}

/// 向当前焦点应用注入 Unicode 文本，返回实际发送的事件数
unsafe fn send_unicode_text(text: &str) -> u32 {
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

    if inputs.is_empty() { return 0; }
    SendInput(&inputs, std::mem::size_of::<INPUT>() as i32)
}


// ============================================================
// 候选词刷新 + 多策略光标定位
// ============================================================

unsafe fn refresh_candidates(state: &mut ImeState) {
    if state.input.engine.is_empty() {
        state.cand_win.hide();
        return;
    }

    let raw = state.input.engine.raw_input().to_string();

    let final_cands = if state.ai.ai_first && state.ai.is_available() {
        // === AI 主导模式 ===
        // AI 直接预测候选, 字典兜底
        let mut ai_cands = state.ai.predict(&raw, &state.history, state.cfg.ai.top_k);
        if ai_cands.is_empty() {
            // AI 无结果 → 回退字典
            let dict_cands = state.input.engine.get_candidates();
            state.plugins.transform_candidates(&raw, dict_cands)
        } else {
            // AI 有结果, 补充字典候选 (去重)
            let dict_cands = state.input.engine.get_candidates();
            let dict_after = state.plugins.transform_candidates(&raw, dict_cands);
            for d in dict_after {
                if !ai_cands.contains(&d) {
                    ai_cands.push(d);
                }
                if ai_cands.len() >= 9 { break; }
            }
            ai_cands
        }
    } else {
        // === 字典主导模式 ===
        // 字典 → 插件 → AI 重排
        let cands = state.input.engine.get_candidates();
        let after_plugins = state.plugins.transform_candidates(&raw, cands);
        state.ai.rerank(&raw, after_plugins, &state.history)
    };

    let count = std::cmp::min(9, final_cands.len());
    if count == 0 { state.cand_win.hide(); return; }

    state.current_candidates = final_cands[..count].to_vec();
    let refs: Vec<&str> = state.current_candidates.iter().map(|s| s.as_str()).collect();
    state.cand_win.update_candidates(&raw, &refs);

    let pt = get_caret_screen_pos();
    state.cand_win.show(pt.x, pt.y + 4);
    let mode = if state.ai.ai_first { "AI主导" } else { "字典+AI" };
    eprintln!("[IME] pinyin={:?}  cands={}  mode={}  pos=({},{})",
        raw, count, mode, pt.x, pt.y + 4);
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
