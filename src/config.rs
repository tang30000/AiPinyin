//! # 配置管理
//!
//! 从 exe 同目录的 `config.toml` 加载用户配置。
//! 文件不存在时使用默认值。

use serde::Deserialize;
use std::path::PathBuf;

/// 顶层配置
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub engine: EngineConfig,
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub dict: DictConfig,
}

/// 引擎模式
#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EngineMode {
    Ai,
    Dict,
}

impl Default for EngineMode {
    fn default() -> Self { EngineMode::Ai }
}

/// 引擎配置
#[derive(Debug, Deserialize, Clone)]
pub struct EngineConfig {
    #[serde(default)]
    pub mode: EngineMode,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self { mode: EngineMode::Ai }
    }
}

/// AI 配置
#[derive(Debug, Deserialize, Clone)]
pub struct AiConfig {
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// AI 是否参与字典候选排序
    #[serde(default)]
    pub rerank: bool,
    /// AI 服务地址（空 = 使用本地内嵌服务）
    /// 兼容任意 OpenAI /v1/chat/completions 接口，如:
    ///   http://localhost:11434/v1  (Ollama)
    ///   https://api.openai.com/v1 (ChatGPT)
    #[serde(default)]
    pub endpoint: String,
    /// 外部 AI 服务 API Key（本地服务留空）
    #[serde(default)]
    pub api_key: String,
    /// 发送给 AI 的系统提示词（空 = 使用内置默认中文提示词）
    #[serde(default)]
    pub system_prompt: String,
}

fn default_top_k() -> usize { 9 }

fn default_system_prompt() -> &'static str {
    "你是拼音输入法候选词排序助手。根据上下文和拼音，从候选列表中选出最合适的词语并排序。\
每行输出一个词语，可选带分数（格式：词语:分数），分数为浮点数，分值越高越优先。\
若不确定分数，直接输出词语即可，按优先级从高到低排列。"
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            top_k: default_top_k(),
            rerank: false,
            endpoint: String::new(),
            api_key: String::new(),
            system_prompt: String::new(),
        }
    }
}


/// UI 配置
#[derive(Debug, Deserialize, Clone)]
pub struct UiConfig {
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default = "default_opacity")]
    pub opacity: u8,
}

fn default_font_size() -> u32 { 16 }
fn default_opacity() -> u8 { 240 }

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            font_size: default_font_size(),
            opacity: default_opacity(),
        }
    }
}

/// 字典配置
#[derive(Debug, Deserialize, Clone)]
pub struct DictConfig {
    /// 额外加载的字典名 (从 dict/ 目录加载, 不含 .txt 后缀)
    #[serde(default)]
    pub extra: Vec<String>,
}

impl Default for DictConfig {
    fn default() -> Self {
        Self { extra: vec![] }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            engine: EngineConfig::default(),
            ai: AiConfig::default(),
            ui: UiConfig::default(),
            dict: DictConfig::default(),
        }
    }
}

impl Config {
    /// 从 exe 同目录加载 config.toml，不存在则用默认值
    pub fn load() -> Self {
        let config_path = Self::config_path();
        match std::fs::read_to_string(&config_path) {
            Ok(text) => {
                match toml::from_str::<Config>(&text) {
                    Ok(cfg) => {
                        eprintln!("[Config] ✅ 已加载 {:?}", config_path);
                        eprintln!("[Config]   mode={:?}, top_k={}, rerank={}, font={}",
                            cfg.engine.mode, cfg.ai.top_k, cfg.ai.rerank, cfg.ui.font_size);
                        if !cfg.dict.extra.is_empty() {
                            eprintln!("[Config]   extra dicts: {:?}", cfg.dict.extra);
                        }
                        cfg
                    }
                    Err(e) => {
                        eprintln!("[Config] ⚠ 解析失败: {}, 使用默认配置", e);
                        Config::default()
                    }
                }
            }
            Err(_) => {
                eprintln!("[Config] ℹ config.toml 不存在, 使用默认配置");
                Config::default()
            }
        }
    }

    fn config_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("config.toml")))
            .unwrap_or_else(|| PathBuf::from("config.toml"))
    }
}
