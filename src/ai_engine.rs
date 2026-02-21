//! # AI 推理引擎
//!
//! 基于 ONNX Runtime 的候选词智能重排引擎。
//!
//! ## 架构
//! - 加载 weights.onnx（自训练轻量 Transformer-Encoder, ~100MB）
//! - 语义排序：根据上下文为重码候选词打分
//! - 智能整句：拼音序列 → Encoder → 汉字 Logits 概率（后续）
//! - 模型不存在时自动回退字典模式
//!
//! ## 张量接口（自训练模型规范）
//! - input_ids:   [1, seq_len]   i64  拼音序列编码
//! - context_ids: [1, ctx_len]   i64  上下文汉字编码
//! - 输出:        [1, vocab_size] f32  汉字概率分布

use std::path::{Path, PathBuf};

// ============================================================
// 上下文缓冲区 — 记录最近上屏的汉字
// ============================================================

/// 历史上屏缓冲区（环形，固定容量）
pub struct HistoryBuffer {
    buf: Vec<String>,
    capacity: usize,
}

impl HistoryBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// 记录一次上屏
    pub fn push(&mut self, text: &str) {
        if text.is_empty() { return; }
        if self.buf.len() >= self.capacity {
            self.buf.remove(0);
        }
        self.buf.push(text.to_string());
    }

    /// 获取最近 N 个上屏文本
    pub fn recent(&self, n: usize) -> Vec<&str> {
        let start = self.buf.len().saturating_sub(n);
        self.buf[start..].iter().map(|s| s.as_str()).collect()
    }

    /// 拼接上下文字符串
    pub fn context_string(&self) -> String {
        self.buf.join("")
    }

    pub fn context_chars(&self) -> usize {
        self.buf.iter().map(|s| s.chars().count()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

// ============================================================
// AI 预测器 — ONNX Runtime v2 封装
// ============================================================

/// AI 推理引擎状态
pub enum AIState {
    /// 模型已加载
    Ready(ort::session::Session),
    /// 回退字典模式
    Unavailable(String),
}

/// 拼音/汉字词表索引（从 JSON 加载）
pub struct VocabIndex {
    pub pinyin2id: std::collections::HashMap<String, i64>,
    pub char2id: std::collections::HashMap<String, i64>,
    pub id2char: std::collections::HashMap<i64, String>,
    pub max_pinyin_len: usize,
    pub max_context_len: usize,
}

impl VocabIndex {
    fn load_from_dir(dir: &std::path::Path) -> Option<Self> {
        let py_path = dir.join("pinyin2id.json");
        let ch_path = dir.join("char2id.json");
        let meta_path = dir.join("vocab_meta.json");

        if !py_path.exists() || !ch_path.exists() {
            eprintln!("[AI] vocab files not found in {:?}", dir);
            return None;
        }

        let py_text = std::fs::read_to_string(&py_path).ok()?;
        let ch_text = std::fs::read_to_string(&ch_path).ok()?;

        let pinyin2id: std::collections::HashMap<String, i64> =
            serde_json::from_str(&py_text).ok()?;
        let char2id: std::collections::HashMap<String, i64> =
            serde_json::from_str(&ch_text).ok()?;

        let id2char: std::collections::HashMap<i64, String> =
            char2id.iter().map(|(k, v)| (*v, k.clone())).collect();

        let (max_pinyin_len, max_context_len) = if meta_path.exists() {
            let meta_text = std::fs::read_to_string(&meta_path).ok()?;
            let meta: serde_json::Value = serde_json::from_str(&meta_text).ok()?;
            (
                meta.get("max_pinyin_len").and_then(|v| v.as_u64()).unwrap_or(32) as usize,
                meta.get("max_context_len").and_then(|v| v.as_u64()).unwrap_or(64) as usize,
            )
        } else {
            (32, 64)
        };

        eprintln!("[AI] vocab: {} pinyin, {} chars, py_max={}, ctx_max={}",
            pinyin2id.len(), char2id.len(), max_pinyin_len, max_context_len);
        Some(VocabIndex { pinyin2id, char2id, id2char, max_pinyin_len, max_context_len })
    }
}

/// AI 候选词重排器
pub struct AIPredictor {
    state: AIState,
    vocab: Option<VocabIndex>,
    model_path: PathBuf,
}

impl AIPredictor {
    /// 尝试加载模型 + 词表，失败时静默回退
    /// 使用 catch_unwind 防止 ort load-dynamic 找不到 DLL 时 panic 导致闪退
    pub fn new() -> Self {
        match std::panic::catch_unwind(|| Self::try_init()) {
            Ok(predictor) => predictor,
            Err(_) => {
                eprintln!("[AI] ⚠ ort 初始化 panic (可能缺少 onnxruntime.dll), 回退字典模式");
                Self {
                    state: AIState::Unavailable("ort panic (missing onnxruntime.dll?)".into()),
                    vocab: None,
                    model_path: PathBuf::new(),
                }
            }
        }
    }

    fn try_init() -> Self {
        let model_path = find_model_path();
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));

        // 加载词表
        let vocab = exe_dir.as_ref().and_then(|d| VocabIndex::load_from_dir(d));

        let state = match &model_path {
            Some(path) => match load_model(path) {
                Ok(session) => {
                    eprintln!("[AI] \u{2705} model loaded: {:?}", path);
                    log_model_info(&session);
                    AIState::Ready(session)
                }
                Err(e) => {
                    eprintln!("[AI] \u{26a0} {}", e);
                    AIState::Unavailable(e)
                }
            },
            None => {
                eprintln!("[AI] \u{2139} weights.onnx not found, dict-only mode");
                AIState::Unavailable("weights.onnx not found".into())
            }
        };

        Self {
            state,
            vocab,
            model_path: model_path.unwrap_or_default(),
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self.state, AIState::Ready(_)) && self.vocab.is_some()
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }
    /// 重排候选词: 字典候选 → AI 语义重排
    pub fn rerank(
        &mut self,
        pinyin: &str,
        candidates: Vec<String>,
        context: &HistoryBuffer,
    ) -> Vec<String> {
        let session = match &mut self.state {
            AIState::Ready(s) => s,
            AIState::Unavailable(_) => return candidates,
        };
        let vocab = match &self.vocab {
            Some(v) => v,
            None => return candidates,
        };

        match run_inference(session, vocab, pinyin, &candidates, context) {
            Ok(scores) => {
                let mut indexed: Vec<(usize, f32)> =
                    scores.into_iter().enumerate().collect();
                indexed.sort_by(|a, b|
                    b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                indexed.into_iter()
                    .filter_map(|(i, _)| candidates.get(i).cloned())
                    .collect()
            }
            Err(e) => {
                eprintln!("[AI] inference error: {}, fallback", e);
                candidates
            }
        }
    }
}

/// ONNX 推理 (free function to avoid borrow conflicts)
fn run_inference(
    session: &mut ort::session::Session,
    vocab: &VocabIndex,
    pinyin: &str,
    candidates: &[String],
    context: &HistoryBuffer,
) -> Result<Vec<f32>, String> {
    // 1. 编码拼音
    let syllables = crate::pinyin::split_pinyin_pub(pinyin);
    let mut py_ids = vec![0i64; vocab.max_pinyin_len];
    for (i, syl) in syllables.iter().enumerate().take(vocab.max_pinyin_len) {
        py_ids[i] = *vocab.pinyin2id.get(syl.as_str()).unwrap_or(&1);
    }

    // 2. 编码上下文
    let ctx_str = context.context_string();
    let mut ctx_ids = vec![0i64; vocab.max_context_len];
    for (i, ch) in ctx_str.chars().rev().enumerate().take(vocab.max_context_len) {
        let idx = vocab.max_context_len - 1 - i;
        ctx_ids[idx] = *vocab.char2id.get(&ch.to_string()).unwrap_or(&1);
    }

    // 3. 创建 ort Tensor
    let py_value = ort::value::Tensor::from_array(
        ([1usize, vocab.max_pinyin_len], py_ids)
    ).map_err(|e| format!("py tensor: {}", e))?;
    let ctx_value = ort::value::Tensor::from_array(
        ([1usize, vocab.max_context_len], ctx_ids)
    ).map_err(|e| format!("ctx tensor: {}", e))?;

    // 4. 运行推理
    let outputs = session.run(ort::inputs![py_value, ctx_value])
        .map_err(|e| format!("session.run: {}", e))?;

    // 5. 提取 logits [1, vocab_size]
    let (_shape, logits_data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("extract logits: {}", e))?;

    // 6. 为每个候选词提取分数
    let scores: Vec<f32> = candidates.iter().map(|cand| {
        if let Some(first_char) = cand.chars().next() {
            if let Some(&char_id) = vocab.char2id.get(&first_char.to_string()) {
                let idx = char_id as usize;
                if idx < logits_data.len() {
                    return logits_data[idx];
                }
            }
        }
        f32::NEG_INFINITY
    }).collect();

    Ok(scores)
}


// ============================================================
// 辅助函数
// ============================================================

/// 查找 weights.onnx
fn find_model_path() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    if let Some(dir) = &exe_dir {
        let p = dir.join("weights.onnx");
        if p.exists() { return Some(p); }
        let p = dir.join("models").join("weights.onnx");
        if p.exists() { return Some(p); }
    }

    let p = PathBuf::from("weights.onnx");
    if p.exists() { Some(p) } else { None }
}

/// 加载 ONNX 模型 (ort v2 API)
fn load_model(path: &Path) -> Result<ort::session::Session, String> {
    eprintln!("[AI] loading {:?} ...", path);
    let start = std::time::Instant::now();

    // ort v2: 直接用 Session::builder()
    let session = ort::session::Session::builder()
        .map_err(|e| format!("session builder: {}", e))?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
        .map_err(|e| format!("optimization: {}", e))?
        .with_intra_threads(2)
        .map_err(|e| format!("threads: {}", e))?
        .commit_from_file(path)
        .map_err(|e| format!("load model: {}", e))?;

    eprintln!("[AI] loaded in {:?}", start.elapsed());
    Ok(session)
}

/// 打印模型输入输出信息
fn log_model_info(session: &ort::session::Session) {
    eprintln!("[AI] inputs: {}, outputs: {}",
        session.inputs().len(), session.outputs().len());
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_buffer() {
        let mut h = HistoryBuffer::new(3);
        assert!(h.is_empty());

        h.push("\u{4f60}");
        h.push("\u{597d}");
        h.push("\u{4e16}");
        assert_eq!(h.context_string(), "\u{4f60}\u{597d}\u{4e16}");
        assert_eq!(h.recent(2), vec!["\u{597d}", "\u{4e16}"]);

        // 超过容量时移除最早的
        h.push("\u{754c}");
        assert_eq!(h.context_string(), "\u{597d}\u{4e16}\u{754c}");
    }

    #[test]
    fn test_ai_fallback() {
        let mut ai = AIPredictor::new();
        // CI 环境下无 onnxruntime DLL 且无模型文件
        assert!(!ai.is_available());

        let history = HistoryBuffer::new(10);
        let cands = vec!["\u{662f}".into(), "\u{65f6}".into(), "\u{5341}".into()];
        let result = ai.rerank("shi", cands.clone(), &history);
        assert_eq!(result, cands);
    }
}
