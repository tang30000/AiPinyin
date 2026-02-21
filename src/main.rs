//! # AiPinyin - AI 驱动的轻量级本地拼音输入法
//!
//! 基于 Windows TSF (Text Services Framework) 框架，
//! 使用 ONNX Runtime 加载本地 AI 权重进行拼音-汉字映射。

mod guardian;

use anyhow::Result;
use log::info;
use windows::core::*;
use windows::Win32::UI::TextServices::*;

// ============================================================
// AiPinyin TSF 文本服务 - COM 类定义
// ============================================================

/// AiPinyin 输入法的核心 COM 类
///
/// 实现 TSF 框架所需的接口，作为输入法引擎的入口。
/// 通过 COM 注册后，系统会在用户切换到此输入法时实例化此类。
#[derive(Debug)]
pub struct AiPinyinTextService {
    /// TSF 线程管理器引用，用于与 TSF 框架交互
    thread_mgr: Option<ITfThreadMgr>,
    /// TSF 分配给此输入法的客户端 ID
    client_id: u32,
    /// 输入法是否已激活
    activated: bool,
}

impl AiPinyinTextService {
    /// 创建新的 AiPinyin 文本服务实例
    pub fn new() -> Self {
        info!("[AiPinyin] 文本服务实例已创建");
        Self {
            thread_mgr: None,
            client_id: 0,
            activated: false,
        }
    }

    /// 激活输入法
    ///
    /// 当用户切换到 AiPinyin 时由 TSF 框架调用。
    /// 负责初始化候选窗口、加载 AI 模型等。
    pub fn activate(&mut self, thread_mgr: ITfThreadMgr, client_id: u32) -> Result<()> {
        info!("[AiPinyin] 正在激活输入法... (client_id: {})", client_id);

        self.thread_mgr = Some(thread_mgr);
        self.client_id = client_id;
        self.activated = true;

        // TODO: 初始化候选窗口 UI
        // TODO: 加载 ONNX 模型权重
        // TODO: 注册按键事件接收器

        info!("[AiPinyin] ✅ 输入法激活成功");
        Ok(())
    }

    /// 停用输入法
    ///
    /// 当用户切换离开 AiPinyin 时由 TSF 框架调用。
    /// 负责清理资源、释放 COM 引用。
    pub fn deactivate(&mut self) -> Result<()> {
        info!("[AiPinyin] 正在停用输入法...");

        // TODO: 关闭候选窗口
        // TODO: 卸载 AI 模型释放内存
        // TODO: 注销按键事件接收器

        self.thread_mgr = None;
        self.client_id = 0;
        self.activated = false;

        info!("[AiPinyin] ✅ 输入法已停用");
        Ok(())
    }

    /// 处理按键输入
    ///
    /// 将用户按键转为拼音序列，送入 AI 引擎推理，返回候选汉字。
    pub fn on_key_event(&self, _key_code: u32) -> Result<Vec<String>> {
        // TODO: 拼音解析
        // TODO: 调用 ONNX 推理引擎
        // TODO: 向量语义匹配，生成候选列表
        Ok(vec![])
    }
}

// ============================================================
// CLSID & 服务描述
// ============================================================

/// AiPinyin 的 COM 类标识符 (CLSID)
///
/// 用于在 Windows 注册表中唯一标识此输入法的 COM 组件。
/// 使用 `uuidgen` 或在线工具生成，确保全局唯一。
pub const CLSID_AIPINYIN: GUID = GUID::from_u128(0xe0e55f04_f427_45f7_86a1_ac150445bcde);

/// 输入法显示信息
pub const TEXTSERVICE_DESC: &str = "AiPinyin 爱拼音输入法";
pub const TEXTSERVICE_MODEL: &str = "AiPinyin Language Model v0.1";

// ============================================================
// 主入口 - 开发调试用
// ============================================================

fn main() -> Result<()> {
    // 初始化日志
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    println!();
    println!("  ╔══════════════════════════════════════════╗");
    println!("  ║    AiPinyin 爱拼音 v{}          ║", env!("CARGO_PKG_VERSION"));
    println!("  ║    AI驱动 · 向量引擎 · 本地推理          ║");
    println!("  ╚══════════════════════════════════════════╝");
    println!();

    info!("AiPinyin v{} 启动中...", env!("CARGO_PKG_VERSION"));

    // 启动 ctfmon.exe 守护线程
    info!("正在启动 Guardian 守护线程...");
    let _guardian_handle = guardian::start_guardian(
        guardian::GuardianConfig::default()
    );
    info!("✅ Guardian 守护线程已启动");

    // 创建文本服务实例（调试用）
    let service = AiPinyinTextService::new();
    info!("输入法服务状态: activated={}", service.activated);

    // 保持主线程运行
    info!("输入法服务运行中... 按 Enter 键退出");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    info!("AiPinyin 已退出");
    Ok(())
}
