//! # 键盘事件处理模块
//!
//! 实现 ITfKeyEventSink 接口，处理按键→拼音→候选的核心逻辑。

use std::cell::RefCell;
use log::info;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::UI::TextServices::*;
use crate::pinyin::PinyinEngine;


// ============================================================
// InputState — 输入状态
// ============================================================

pub struct InputState {
    pub engine: PinyinEngine,
    pub committed: String,
}

impl InputState {
    pub fn new() -> Self {
        Self { engine: PinyinEngine::new(), committed: String::new() }
    }
}

// ============================================================
// 核心按键处理逻辑
// ============================================================

pub struct KeyResult {
    pub eaten: bool,
    /// 需要上屏的动作
    pub commit: Option<CommitAction>,
    pub need_refresh: bool,
}

/// 上屏动作
pub enum CommitAction {
    /// 上屏当前显示候选列表中的第 i 项（由调用方从缓存中取）
    Index(usize),
    /// 直接上屏指定文本（Enter 原始字母）
    Text(String),
}

pub fn handle_key_down(state: &mut InputState, vkey: u32) -> KeyResult {
    match vkey {
        // A-Z
        0x41..=0x5A => {
            let ch = (vkey as u8 + 32) as char;
            state.engine.push(ch);
            info!("[Key] '{}' → {:?}", ch, state.engine.syllables());
            KeyResult { eaten: true, commit: None, need_refresh: true }
        }
        // Backspace
        0x08 => {
            if state.engine.is_empty() {
                KeyResult { eaten: false, commit: None, need_refresh: false }
            } else {
                state.engine.pop();
                KeyResult { eaten: true, commit: None, need_refresh: true }
            }
        }
        // Space → 选第一个（索引 0）
        0x20 => {
            if state.engine.is_empty() {
                KeyResult { eaten: false, commit: None, need_refresh: false }
            } else {
                // 不在这里 clear，由 main.rs 根据选中词的字数决定消耗几个音节
                KeyResult { eaten: true, commit: Some(CommitAction::Index(0)), need_refresh: true }
            }
        }
        // 1-9 → 选对应索引
        0x31..=0x39 => {
            if state.engine.is_empty() {
                KeyResult { eaten: false, commit: None, need_refresh: false }
            } else {
                let idx = (vkey - 0x31) as usize;
                // 不在这里 clear，由 main.rs 根据选中词的字数决定消耗几个音节
                KeyResult { eaten: true, commit: Some(CommitAction::Index(idx)), need_refresh: true }
            }
        }
        // Escape → 取消，不输出任何内容
        0x1B => {
            if state.engine.is_empty() {
                KeyResult { eaten: false, commit: None, need_refresh: false }
            } else {
                state.engine.clear();
                KeyResult { eaten: true, commit: None, need_refresh: true }
            }
        }
        // Enter → 以原始字母形式上屏
        0x0D => {
            if state.engine.is_empty() {
                KeyResult { eaten: false, commit: None, need_refresh: false }
            } else {
                let raw = state.engine.raw_input().to_string();
                state.engine.clear();
                KeyResult { eaten: true, commit: Some(CommitAction::Text(raw)), need_refresh: true }
            }
        }
        _ => KeyResult { eaten: false, commit: None, need_refresh: false },
    }
}

// ============================================================
// ITfKeyEventSink COM 实现
// ============================================================

#[implement(ITfKeyEventSink)]
pub struct AiPinyinKeyEventSink {
    state: RefCell<InputState>,
}

impl AiPinyinKeyEventSink {
    pub fn new() -> Self {
        Self { state: RefCell::new(InputState::new()) }
    }
}

// 方法签名: pfEaten 是返回值 Result<BOOL>，不是参数
impl ITfKeyEventSink_Impl for AiPinyinKeyEventSink_Impl {
    fn OnSetFocus(&self, _fforeground: BOOL) -> Result<()> {
        Ok(())
    }

    fn OnTestKeyDown(
        &self, _pic: Option<&ITfContext>, wparam: WPARAM, _lparam: LPARAM,
    ) -> Result<BOOL> {
        let state = self.state.borrow();
        let eat = match wparam.0 as u32 {
            0x41..=0x5A => true,
            0x08 | 0x0D | 0x20 | 0x1B => !state.engine.is_empty(),
            0x31..=0x39 => !state.engine.is_empty(),
            _ => false,
        };
        Ok(BOOL::from(eat))
    }

    fn OnTestKeyUp(
        &self, _pic: Option<&ITfContext>, _wparam: WPARAM, _lparam: LPARAM,
    ) -> Result<BOOL> {
        Ok(FALSE)
    }

    fn OnKeyDown(
        &self, _pic: Option<&ITfContext>, wparam: WPARAM, _lparam: LPARAM,
    ) -> Result<BOOL> {
        let mut state = self.state.borrow_mut();
        let result = handle_key_down(&mut state, wparam.0 as u32);
        Ok(BOOL::from(result.eaten))
    }

    fn OnKeyUp(
        &self, _pic: Option<&ITfContext>, _wparam: WPARAM, _lparam: LPARAM,
    ) -> Result<BOOL> {
        Ok(FALSE)
    }

    fn OnPreservedKey(
        &self, _pic: Option<&ITfContext>, _rguid: *const GUID,
    ) -> Result<BOOL> {
        Ok(FALSE)
    }
}
