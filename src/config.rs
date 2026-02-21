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
}

fn default_top_k() -> usize { 9 }

impl Default for AiConfig {
    fn default() -> Self {
        Self { top_k: default_top_k() }
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

impl Default for Config {
    fn default() -> Self {
        Self {
            engine: EngineConfig::default(),
            ai: AiConfig::default(),
            ui: UiConfig::default(),
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
                        eprintln!("[Config]   mode={:?}, top_k={}, font={}",
                            cfg.engine.mode, cfg.ai.top_k, cfg.ui.font_size);
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
