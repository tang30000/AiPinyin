//! # AiPinyin — AI 驱动的轻量级本地拼音输入法
//!
//! 基于 Windows TSF 框架，使用本地 AI 权重进行拼音-汉字映射。

mod guardian;
pub mod ui;

use anyhow::Result;
use log::info;
use windows::core::*;
use windows::Win32::UI::TextServices::*;

// ============================================================
// AiPinyin TSF 文本服务 - COM 类定义
// ============================================================

/// AiPinyin 输入法的核心 COM 类
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
        info!("[AiPinyin] 激活输入法 (client_id: {})", client_id);
        self.thread_mgr = Some(thread_mgr);
        self.client_id = client_id;
        self.activated = true;
        Ok(())
    }

    pub fn deactivate(&mut self) -> Result<()> {
        info!("[AiPinyin] 停用输入法");
        self.thread_mgr = None;
        self.client_id = 0;
        self.activated = false;
        Ok(())
    }
}

// ============================================================
// CLSID & 描述
// ============================================================

pub const CLSID_AIPINYIN: GUID = GUID::from_u128(0xe0e55f04_f427_45f7_86a1_ac150445bcde);
pub const TEXTSERVICE_DESC: &str = "AiPinyin 爱拼音输入法";

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

    // 启动 Guardian 守护线程
    let _guardian = guardian::start_guardian(guardian::GuardianConfig::default());
    info!("✅ Guardian 守护线程已启动");

    // 创建并展示候选词窗口 (演示)
    info!("正在创建候选词窗口...");
    let window = ui::CandidateWindow::new()?;

    // 演示数据
    window.draw_candidates(&["爱", "埃", "碍", "矮", "哎", "挨", "癌", "蔼"]);
    window.show(500, 400);
    info!("✅ 候选词窗口已显示 (按 ESC 退出)");

    // 运行消息循环
    ui::run_message_loop();

    info!("AiPinyin 已退出");
    Ok(())
}
