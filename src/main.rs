//! # AiPinyin — AI 驱动的轻量级本地拼音输入法
//!
//! 基于 Windows TSF 框架，使用本地 AI 权重进行拼音-汉字映射。

mod guardian;
pub mod pinyin;
pub mod ui;

use anyhow::Result;
use log::info;
use windows::core::*;
use windows::Win32::UI::TextServices::*;

// ============================================================
// AiPinyin TSF 文本服务 - COM 类定义
// ============================================================

#[derive(Debug)]
pub struct AiPinyinTextService {
    thread_mgr: Option<ITfThreadMgr>,
    client_id: u32,
    activated: bool,
}

impl AiPinyinTextService {
    pub fn new() -> Self {
        Self { thread_mgr: None, client_id: 0, activated: false }
    }

    pub fn activate(&mut self, thread_mgr: ITfThreadMgr, client_id: u32) -> Result<()> {
        info!("[TSF] 激活输入法 (client_id: {})", client_id);
        self.thread_mgr = Some(thread_mgr);
        self.client_id = client_id;
        self.activated = true;
        Ok(())
    }

    pub fn deactivate(&mut self) -> Result<()> {
        info!("[TSF] 停用输入法");
        self.thread_mgr = None;
        self.client_id = 0;
        self.activated = false;
        Ok(())
    }
}

pub const CLSID_AIPINYIN: GUID = GUID::from_u128(0xe0e55f04_f427_45f7_86a1_ac150445bcde);

// ============================================================
// 主入口 — 演示拼音引擎 + 候选词窗口联动
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

    // 启动 Guardian 守护线程
    let _guardian = guardian::start_guardian(guardian::GuardianConfig::default());
    info!("✅ Guardian 守护线程已启动");

    // 演示拼音引擎
    let mut engine = pinyin::PinyinEngine::new();
    for ch in "shi".chars() { engine.push(ch); }
    info!("拼音输入: \"{}\" → 音节: {:?}", engine.raw_input(), engine.syllables());

    let candidates = engine.get_candidates();
    info!("候选词: {:?}", &candidates[..std::cmp::min(8, candidates.len())]);

    // 创建候选词窗口
    let window = ui::CandidateWindow::new()?;

    // 将候选词显示到窗口
    let cand_refs: Vec<&str> = candidates.iter().map(|s| s.as_str()).collect();
    window.draw_candidates(&cand_refs[..std::cmp::min(9, cand_refs.len())]);
    window.show(500, 400);
    info!("✅ 候选词窗口已显示 (按 ESC 退出)");

    // 运行消息循环
    ui::run_message_loop();

    info!("AiPinyin 已退出");
    Ok(())
}
