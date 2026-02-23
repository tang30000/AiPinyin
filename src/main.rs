//! # AiPinyin — AI 驱动的轻量级本地拼音输入法
//!
//! 架构：WH_KEYBOARD_LL 全局键盘钩子 + 多策略光标定位

mod guardian;
pub mod ai_engine;
pub mod ai_server;
pub mod config;
pub mod key_event;
pub mod pinyin;
pub mod plugin_system;
pub mod user_dict;
pub mod settings;
pub mod webview_ui;


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
    cand_win: Option<webview_ui::WebViewUI>,
    plugins: plugin_system::PluginSystem,
    ai: ai_engine::AIPredictor,
    history: ai_engine::HistoryBuffer,
    cfg: config::Config,
    user_dict: user_dict::UserDict,
    /// 本地 AI 服务实际监听端口（0 = 服务未启动）
    ai_port: u16,
    /// 最终使用的 AI endpoint（本地或用户配置的外部地址）
    ai_endpoint: String,
    current_candidates: Vec<String>,
    all_candidates: Vec<String>,
    page_offset: usize,
    chinese_mode: bool,
    shift_down: bool,
    shift_modified: bool,
    ai_generation: u64,
    last_commit: Option<(String, String)>,
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

    // 初始化 AI 推理引擎（Arc<Mutex<>> 共享给本地 HTTP 服务线程）
    let ai_arc = std::sync::Arc::new(std::sync::Mutex::new(ai_engine::AIPredictor::new()));
    {
        let mut pred = ai_arc.lock().unwrap();
        pred.ai_first = cfg.engine.mode == config::EngineMode::Ai;
    }
    let history_arc = std::sync::Arc::new(std::sync::Mutex::new(
        ai_engine::HistoryBuffer::new(100)
    ));

    // 确定 ui/ 目录（向 ai_server 提供静态文件服务）
    let ui_dir_dev = std::path::PathBuf::from("ui");
    let ui_dir_exe = std::env::current_exe()
        .ok().and_then(|p| p.parent().map(|d| d.join("ui"))).unwrap_or_default();
    let ui_dir = if ui_dir_dev.exists() {
        Some(ui_dir_dev)
    } else if ui_dir_exe.exists() {
        Some(ui_dir_exe)
    } else {
        None
    };

    // 启动本地 AI HTTP 服务（也提供 UI 静态文件）
    let system_prompt = cfg.ai.system_prompt.clone();
    let ai_port = ai_server::start(
        std::sync::Arc::clone(&ai_arc),
        std::sync::Arc::clone(&history_arc),
        ui_dir,
        system_prompt,
    );

    // main 线程保留一份 AI 实例，用于同步降级
    let mut ai = ai_engine::AIPredictor::new();
    ai.ai_first = cfg.engine.mode == config::EngineMode::Ai;
    let history = ai_engine::HistoryBuffer::new(100);

    // 确定最终 AI endpoint
    let ai_endpoint = if !cfg.ai.endpoint.is_empty() {
        cfg.ai.endpoint.clone()
    } else if ai_port > 0 {
        format!("http://127.0.0.1:{}/v1", ai_port)
    } else {
        String::new()
    };

    // Load webview ui instance（传入 ai_port 以便 UI 用 http:// 加载）
    let (cand_win_ui, event_loop) = webview_ui::WebViewUI::new()?;

    let user_dict = user_dict::UserDict::load();

    let state = Box::new(ImeState {
        input: InputState::new(),
        cand_win: Some(cand_win_ui),
        plugins,
        ai,
        history,
        cfg,
        user_dict,
        ai_port,
        ai_endpoint,
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

        // 初始化 [JS] 按钮状态
        let s = &mut *GLOBAL_STATE;
        if let Some(cw) = &s.cand_win {
            cw.set_plugins_active(s.plugins.has_active());
        }

        let hinstance = GetModuleHandleW(None)?;
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_hook),
            hinstance,
            0,
        )?;
        println!("  ✅ 全局钩子已安装，请切换到其他窗口打字...");
        println!("  【Shift】切换中/英文模式");

        // Webview 主循环
        std::thread::spawn(move || {
            // Note: Since tao triggers the loop on main thread we will keep weview running here
        });
        
        webview_ui::run_webview_loop(event_loop, ai_port)?;

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
    if let Some(cw) = &state.cand_win {
        cw.set_plugins_active(state.plugins.has_active());
    }
    result
}

// ============================================================
// 异步按键处理回调（由 wnd_proc 收到 WM_IME_KEYDOWN 后调用）
// ============================================================

unsafe fn cb_process_key(vkey: u32) {
    if GLOBAL_STATE.is_null() { return; }
    let state = &mut *GLOBAL_STATE;

    // 翻页键直接处理
    match vkey {
        0xBB | 0x22 => { page_down(state); return; }
        0xBD | 0x21 => { page_up(state); return; }
        _ => {}
    }

    let raw_before = state.input.engine.raw_input().to_string();
    let result = handle_key_down(&mut state.input, vkey);

    match result.commit {
        Some(CommitAction::Index(idx)) => {
            let text = state.current_candidates.get(idx).cloned().unwrap_or_default();
            if !text.is_empty() {
                state.history.push(&text);
                if !raw_before.is_empty() {
                    state.user_dict.learn(&raw_before, &text);
                    if text.chars().count() >= 3 {
                        crate::pinyin::cache_ai_word(&raw_before, &text);
                    }
                }
                state.last_commit = Some((raw_before.clone(), text.clone()));
                state.backspace_count = 0;
                eprintln!("[IME] ↑ {:?}", text);
                send_unicode_text(&text);

                let char_count = text.chars().count();
                state.input.engine.consume_syllables(char_count);
                state.current_candidates.clear();

                if state.input.engine.is_empty() {
                    state.all_candidates.clear();
                    state.current_candidates.clear();
                    if let Some(cw) = &state.cand_win {
                        cw.hide();
                    }
                } else {
                    refresh_candidates(state);
                }
                return;
            }
        }
        Some(CommitAction::Text(text)) => {
            if let Some(cw) = &state.cand_win {
                cw.hide();
            }
            state.input.engine.clear();
            state.current_candidates.clear();
            state.history.push(&text);
            eprintln!("[IME] ↑ {:?}", text);
            send_unicode_text(&text);
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
            let has_input = !state.input.engine.is_empty();
            let should_eat = match vkey {
                0x41..=0x5A => true,
                0x08 => has_input,
                0x20 => has_input,
                0x31..=0x39 => has_input,
                0x1B => has_input,
                0x0D => has_input,
                0xBB | 0xBD | 0x21 | 0x22 => has_input,
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
                // 给 cb_process_key 线程设置足够大的栈空间，避免 ONNX 推理时栈溢出 (STATUS_STACK_BUFFER_OVERRUN)
                let _ = std::thread::Builder::new()
                    .stack_size(8 * 1024 * 1024) // 8 MB
                    .spawn(move || {
                        cb_process_key(vkey as u32);
                    });
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
        if let Some(cw) = &state.cand_win {
            cw.hide();
        }
        eprintln!("[IME] ⌨  EN → 英文直通（按 Shift 切回中文）");
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
    if total == 0 { 
        if let Some(cw) = &state.cand_win {
            cw.hide(); 
        }
        return; 
    }

    let offset = state.page_offset.min(total.saturating_sub(1));
    let end = std::cmp::min(offset + PAGE_SIZE, total);
    state.current_candidates = state.all_candidates[offset..end].to_vec();

    let page_num = offset / PAGE_SIZE + 1;
    let total_pages = (total + PAGE_SIZE - 1) / PAGE_SIZE;
    let page_info = if total_pages > 1 { Some((page_num, total_pages)) } else { None };

    let refs: Vec<&str> = state.current_candidates.iter().map(|s| s.as_str()).collect();
    if let Some(cw) = &state.cand_win {
        cw.update_candidates_with_page(raw, &refs, page_info);
    }
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
        if let Some(cw) = &state.cand_win {
            cw.hide();
        }
        return;
    }

    let raw = state.input.engine.raw_input().to_string();
    let syllables = state.input.engine.syllables().to_vec();

    // Phase 1: 立即显示候选 (同步, <5ms)
    let dict_cands = state.input.engine.get_candidates();
    let dict_after = state.plugins.transform_candidates(&raw, dict_cands);

    // 改动4: 单音节时同步运行一次 AI 推理（单次推理 <2ms, 用户无感知延迟）
    // 让用户第一时间看到 AI 排序的结果，而不是等待异步更新
    let sync_ai_cands: Vec<String> = if syllables.len() == 1 && state.ai.is_available() {
        let ctx = state.history.context_string();
        state.ai.predict(&raw, &ctx, 9, &dict_after)
    } else {
        vec![]
    };

    // 用户自学习提权 + 合并
    // 改动1: 顺序 = 用户词 → AI词 → 字典词（字典只补充不重复的）
    let display_cands = {
        let learned = state.user_dict.get_learned_words(&raw);
        let mut merged: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // 0. 用户学习词（最高优先级）
        for (word, _) in &learned {
            if seen.insert(word.clone()) { merged.push(word.clone()); }
        }
        // 1. AI 同步推理结果（单音节时）
        for w in &sync_ai_cands {
            if seen.insert(w.clone()) { merged.push(w.clone()); }
        }
        // 2. 字典候选（补充剩余位置）
        for word in &dict_after {
            if seen.insert(word.clone()) { merged.push(word.clone()); }
        }
        merged
    };

    if display_cands.is_empty() { 
        if let Some(cw) = &state.cand_win {
            cw.hide();
        }
        return; 
    }

    // 保存所有候选, 显示当前页
    state.all_candidates = display_cands;
    state.page_offset = 0;
    show_current_page(state, &raw);

    let pt = get_caret_screen_pos();
    if let Some(cw) = &state.cand_win {
        cw.show(pt.x, pt.y + 4);
    }

    // Phase 2: AI 推理在后台线程 (异步, 用于多音节/长句上下文感知更新)
    // 单音节已在 Phase 1 同步处理，这里重点处理多音节和上下文感知重排
    if state.ai.ai_first && state.ai.is_available() {
        let raw_clone = raw.clone();
        let dict_clone = dict_after;
        let ai_top_k = std::cmp::min(state.cfg.ai.top_k, 9);
        
        let hwnd_raw = if let Some(cw) = &state.cand_win {
            cw.hwnd().0 as isize
        } else {
            0
        };

        state.ai_generation += 1;
        let gen = state.ai_generation;

        // 给 AI 推理线程设置足够大的栈空间 (ONNX Runtime beam search 资源开销大)
        let _ = std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024) // 8 MB
            .spawn(move || {
                let state_ptr = GLOBAL_STATE;
                if state_ptr.is_null() { return; }
                let state = &mut *state_ptr;

                let ctx = state.history.context_string();
                let ai_scored = state.ai.predict(
                    &raw_clone, &ctx, ai_top_k, &dict_clone,
                );


                if state.ai_generation != gen { return; }

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
                }

                if let Some(cw) = &state.cand_win {
                    state.all_candidates = merged;
                    state.page_offset = 0;
                    let raw_string = raw_clone;
                    let refs: Vec<&str> = state.all_candidates.iter().take(PAGE_SIZE).map(|s| s.as_str()).collect();
                    let page_info = if state.all_candidates.len() > PAGE_SIZE {
                        Some((1, (state.all_candidates.len() + PAGE_SIZE - 1) / PAGE_SIZE))
                    } else {
                        None
                    };
                    cw.update_candidates_with_page(&raw_string, &refs, page_info);
                    if state.input.engine.is_empty() {
                        let pt = get_caret_screen_pos();
                        cw.show(pt.x, pt.y + 4);
                    }
                }
            });
    }

    eprintln!("[IME] pinyin={:?}  cands={}  mode={}",
        raw, state.all_candidates.len(), if state.ai.ai_first { "AI" } else { "字典" });
}



/// 多策略获取光标屏幕坐标
///
/// 策略1: OBJID_CARET (Accessibility) — 精确屏幕坐标，适用于所有支持 MSAA 的应用
/// 策略2: GetGUIThreadInfo — 旧式 Win32 Caret API（记事本/WordPad 等）
/// 策略3: 鼠标位置 — 通用回退
pub(crate) unsafe fn get_caret_screen_pos() -> POINT {
    use windows::Win32::UI::Accessibility::{
        AccessibleObjectFromWindow, IAccessible,
    };

    let fg = GetForegroundWindow();

    // ── 策略1: Accessibility OBJID_CARET ──────────────────────────────
    // OBJID_CARET = -8i32 (0xFFFFFFF8)
    const OBJID_CARET: u32 = 0xFFFFFFF8u32;
    if !fg.is_invalid() {
        let mut pacc: Option<IAccessible> = None;
        if AccessibleObjectFromWindow(
            fg,
            OBJID_CARET,
            &IAccessible::IID,
            &mut pacc as *mut _ as *mut *mut core::ffi::c_void,
        ).is_ok() {
            if let Some(acc) = pacc {
                let child = windows_core::VARIANT::from(0i32);
                let mut left = 0i32;
                let mut top = 0i32;
                let mut width = 0i32;
                let mut height = 0i32;
                if acc.accLocation(&mut left, &mut top, &mut width, &mut height, &child).is_ok()
                    && (left != 0 || top != 0)
                {
                    return POINT { x: left, y: top + height };
                }
            }
        }
    }

    // ── 策略2: GetGUIThreadInfo (旧式 Win32 Caret) ─────────────────────
    if !fg.is_invalid() {
        let thread_id = GetWindowThreadProcessId(fg, None);
        let mut gi = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread_id, &mut gi).is_ok() && !gi.hwndCaret.is_invalid() {
            let h = gi.rcCaret.bottom - gi.rcCaret.top;
            let w = gi.rcCaret.right - gi.rcCaret.left;
            if h > 0 || w > 0 {
                let mut pt = POINT { x: gi.rcCaret.left, y: gi.rcCaret.bottom };
                let _ = ClientToScreen(gi.hwndCaret, &mut pt);
                // 合理性检验：与鼠标偏差不超过 400px
                let mut mouse = POINT::default();
                let _ = GetCursorPos(&mut mouse);
                if pt.x >= 0 && pt.y >= 0 && (pt.y - mouse.y).abs() < 400 {
                    return pt;
                }
            }
        }
    }

    // ── 策略3: 鼠标光标位置 ────────────────────────────────────────────
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    POINT { x: pt.x, y: pt.y + 20 }
}

