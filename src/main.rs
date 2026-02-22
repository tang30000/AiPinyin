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
pub mod user_dict;
pub mod settings;


use anyhow::Result;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use crate::key_event::{InputState, CommitAction, handle_key_down};

/// 自定义消息: 钩子先拦截按键，然后通过此消息异步处理
const WM_IME_KEYDOWN: u32 = WM_APP + 1;
/// 自定义消息: AI 后台线程完成推理, 通知主线程更新候选
const WM_AI_RESULT: u32 = WM_APP + 2;

/// AI 线程存放结果, 主线程读取
static mut AI_RESULT: Option<(u64, String, Vec<String>)> = None;

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
    user_dict: user_dict::UserDict,
    /// 候选窗口当前显示的候选词（当前页）
    current_candidates: Vec<String>,
    /// 所有候选词（用于翻页）
    all_candidates: Vec<String>,
    /// 当前页偏移（0, 9, 18, ...）
    page_offset: usize,
    chinese_mode: bool,
    shift_down: bool,
    shift_modified: bool,
    /// AI 异步推理代次号, 用于丢弃过期结果
    ai_generation: u64,
    /// 上次上屏的 (拼音, 汉字), 用于检测退格撤销
    last_commit: Option<(String, String)>,
    /// 退格计数: 用户连续按了多少次退格
    backspace_count: usize,
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

    // 初始化字典（基础 + 额外词库）
    pinyin::init_global_dict(&cfg.dict.extra);

    // 初始化 AI 推理引擎
    let mut ai = ai_engine::AIPredictor::new();
    // 应用配置: 引擎模式
    ai.ai_first = cfg.engine.mode == config::EngineMode::Ai;
    let history = ai_engine::HistoryBuffer::new(10);

    let cand_win = ui::CandidateWindow::new()?;
    let user_dict = user_dict::UserDict::load();

    let state = Box::new(ImeState {
        input: InputState::new(),
        cand_win,
        plugins,
        ai,
        history,
        cfg,
        user_dict,
        current_candidates: Vec::new(),
        all_candidates: Vec::new(),
        page_offset: 0,
        chinese_mode: true,
        shift_down: false,
        shift_modified: false,
        ai_generation: 0,
        last_commit: None,
        backspace_count: 0,
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

    // 翻页键: 直接处理, 不走 handle_key_down
    match vkey {
        0xBB | 0x22 => { // = 或 PgDn → 下一页
            page_down(state);
            return;
        }
        0xBD | 0x21 => { // - 或 PgUp → 上一页
            page_up(state);
            return;
        }
        _ => {}
    }

    // 保存当前拼音（handle_key_down 可能会 clear）
    let raw_before = state.input.engine.raw_input().to_string();

    // 调用原有的按键处理逻辑
    let result = handle_key_down(&mut state.input, vkey);

    match result.commit {
        Some(CommitAction::Index(idx)) => {
            let text = state.current_candidates.get(idx).cloned()
                .unwrap_or_default();
            if !text.is_empty() {
                state.history.push(&text);  // 记录上屏历史
                // 自学习：记录 (拼音 → 选词)
                if !raw_before.is_empty() {
                    state.user_dict.learn(&raw_before, &text);
                    // 3+字词自动缓存到主字典 (下次词图直接命中)
                    if text.chars().count() >= 3 {
                        crate::pinyin::cache_ai_word(&raw_before, &text);
                    }
                }
                // 记录上次上屏, 用于退格撤销
                state.last_commit = Some((raw_before.clone(), text.clone()));
                state.backspace_count = 0;
                eprintln!("[IME] ↑ 上屏 {:?}  (sent={})", text,
                    send_unicode_text(&text));

                // 部分消耗: 根据选中词的字数消耗对应音节
                let char_count = text.chars().count();
                state.input.engine.consume_syllables(char_count);
                state.current_candidates.clear();

                if state.input.engine.is_empty() {
                    // 全部消耗完 → 隐藏候选窗
                    state.cand_win.hide();
                } else {
                    // 还有剩余音节 → 立即刷新候选
                    eprintln!("[IME] 剩余: {:?} → {:?}",
                        state.input.engine.raw_input(),
                        state.input.engine.syllables());
                    refresh_candidates(state);
                    return;  // 已经 refresh 了, 不要重复
                }
            }
        }
        Some(CommitAction::Text(text)) => {
            state.cand_win.hide();
            state.input.engine.clear();
            state.current_candidates.clear();
            state.history.push(&text);  // 记录上屏历史
            eprintln!("[IME] ↑ 上屏 {:?}  (sent={})", text,
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
                0xBB => !state.input.engine.is_empty(), // = (下一页)
                0xBD => !state.input.engine.is_empty(), // - (上一页)
                0x21 => !state.input.engine.is_empty(), // PgUp
                0x22 => !state.input.engine.is_empty(), // PgDn
                _ => false,
            };

            // 退格撤销: 中文模式、引擎为空、按退格 → 可能在删刚才选错的词
            if vkey == 0x08 && !should_eat && state.chinese_mode {
                if let Some((ref py, ref word)) = state.last_commit.clone() {
                    state.backspace_count += 1;
                    let word_len = word.chars().count();
                    if state.backspace_count >= word_len {
                        // 用户删完了刚才上屏的整个词 → 撤销学习
                        state.user_dict.unlearn(py, word);
                        eprintln!("[IME] ⏪ 撤销学习: {} → {} (退格{}次)",
                            py, word, state.backspace_count);
                        state.last_commit = None;
                        state.backspace_count = 0;
                    }
                }
            } else if vkey != 0x08 {
                // 按了非退格键 → 清除退格追踪
                if state.last_commit.is_some() {
                    state.last_commit = None;
                    state.backspace_count = 0;
                }
            }

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
// 翻页 + 候选词刷新
// ============================================================

const PAGE_SIZE: usize = 9;

/// 显示当前页候选词
pub(crate) unsafe fn show_current_page(state: &mut ImeState, raw: &str) {
    let total = state.all_candidates.len();
    if total == 0 { state.cand_win.hide(); return; }

    let offset = state.page_offset.min(total.saturating_sub(1));
    let end = std::cmp::min(offset + PAGE_SIZE, total);
    state.current_candidates = state.all_candidates[offset..end].to_vec();

    let page_num = offset / PAGE_SIZE + 1;
    let total_pages = (total + PAGE_SIZE - 1) / PAGE_SIZE;
    let page_info = if total_pages > 1 { Some((page_num, total_pages)) } else { None };

    let refs: Vec<&str> = state.current_candidates.iter().map(|s| s.as_str()).collect();
    state.cand_win.update_candidates_with_page(raw, &refs, page_info);
}

/// 下一页
unsafe fn page_down(state: &mut ImeState) {
    let total = state.all_candidates.len();
    if state.page_offset + PAGE_SIZE < total {
        state.page_offset += PAGE_SIZE;
        let raw = state.input.engine.raw_input().to_string();
        show_current_page(state, &raw);
    }
}

/// 上一页
unsafe fn page_up(state: &mut ImeState) {
    if state.page_offset >= PAGE_SIZE {
        state.page_offset -= PAGE_SIZE;
        let raw = state.input.engine.raw_input().to_string();
        show_current_page(state, &raw);
    }
}

unsafe fn refresh_candidates(state: &mut ImeState) {
    if state.input.engine.is_empty() {
        state.cand_win.hide();
        return;
    }

    let raw = state.input.engine.raw_input().to_string();

    // Phase 1: 立即显示字典候选 (同步, <1ms)
    let dict_cands = state.input.engine.get_candidates();
    let dict_after = state.plugins.transform_candidates(&raw, dict_cands);

    // 用户自学习提权
    let display_cands = {
        let learned = state.user_dict.get_learned_words(&raw);
        if learned.is_empty() {
            dict_after.clone()
        } else {
            let mut boosted: Vec<String> = Vec::new();
            for (word, _count) in &learned {
                if !boosted.contains(word) { boosted.push(word.clone()); }
            }
            for word in &dict_after {
                if !boosted.contains(word) { boosted.push(word.clone()); }
            }
            boosted
        }
    };

    if display_cands.is_empty() { state.cand_win.hide(); return; }

    // 保存所有候选, 显示当前页
    state.all_candidates = display_cands;
    state.page_offset = 0;
    show_current_page(state, &raw);

    let pt = get_caret_screen_pos();
    state.cand_win.show(pt.x, pt.y + 4);

    // Phase 2: AI 推理在后台线程 (异步, 不阻塞 UI)
    if state.ai.ai_first && state.ai.is_available() {
        let raw_clone = raw.clone();
        let dict_clone = dict_after;
        let hwnd_raw = state.cand_win.hwnd().0 as isize;  // HWND is !Send, pass as raw
        let ctx_str = state.history.context_string();
        let ai_top_k = std::cmp::min(state.cfg.ai.top_k, 5);

        // 递增 generation, 后续结果只在 generation 匹配时才应用
        state.ai_generation += 1;
        let gen = state.ai_generation;

        std::thread::spawn(move || {
            let state_ptr = GLOBAL_STATE;
            if state_ptr.is_null() { return; }
            let state = &mut *state_ptr;

            let ai_scored = state.ai.predict(
                &raw_clone, &state.history, ai_top_k, &dict_clone,
            );

            // 如果 generation 已变, 丢弃过期结果
            if state.ai_generation != gen { return; }

            // 合并 AI + 字典
            let mut merged = Vec::new();
            let mut seen = std::collections::HashSet::new();
            let learned = state.user_dict.get_learned_words(&raw_clone);
            for (word, _) in &learned {
                if seen.insert(word.clone()) { merged.push(word.clone()); }
            }
            for w in &ai_scored {
                if seen.insert(w.clone()) { merged.push(w.clone()); }
            }
            for w in &dict_clone {
                if seen.insert(w.clone()) { merged.push(w.clone()); }
                if merged.len() >= 15 { break; }
            }

            // 存结果, PostMessage 通知主线程 (UI 操作只能在主线程)
            AI_RESULT = Some((gen, raw_clone, merged));
            let hwnd = HWND(hwnd_raw as *mut _);
            let _ = PostMessageW(hwnd, WM_AI_RESULT, WPARAM(0), LPARAM(0));
        });
    }

    eprintln!("[IME] pinyin={:?}  cands={}  mode={}",
        raw, state.all_candidates.len(), if state.ai.ai_first { "AI异步" } else { "字典" });
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
