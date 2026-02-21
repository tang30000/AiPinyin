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

/// AI 候选词重排器
pub struct AIPredictor {
    state: AIState,
    model_path: PathBuf,
}

impl AIPredictor {
    /// 尝试加载模型，失败时静默回退
    pub fn new() -> Self {
        let model_path = find_model_path();

        let state = match &model_path {
            Some(path) => match load_model(path) {
                Ok(session) => {
                    eprintln!("[AI] \u{2705} \u{6a21}\u{578b}\u{5df2}\u{52a0}\u{8f7d}: {:?}", path);
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
            model_path: model_path.unwrap_or_default(),
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self.state, AIState::Ready(_))
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    /// 重排候选词
    ///
    /// 管线: 字典候选 → AI 语义重排
    /// 失败时原样返回（零干扰回退）
    pub fn rerank(
        &self,
        pinyin: &str,
        candidates: Vec<String>,
        context: &HistoryBuffer,
    ) -> Vec<String> {
        let session = match &self.state {
            AIState::Ready(s) => s,
            AIState::Unavailable(_) => return candidates,
        };

        match self.run_inference(session, pinyin, &candidates, context) {
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

    /// ONNX 推理（预留接口）
    ///
    /// 当前返回占位评分（保持原序），
    /// 等自训练模型完成后替换。
    fn run_inference(
        &self,
        _session: &ort::session::Session,
        _pinyin: &str,
        candidates: &[String],
        _context: &HistoryBuffer,
    ) -> Result<Vec<f32>, String> {
        // ══════════════════════════════════════════════════
        // TODO: 替换为真正的 ONNX 推理
        //
        // 自训练模型规范:
        //   Architecture: Transformer-Encoder (6 layers, d=256, 4 heads)
        //   Input:  pinyin token ids  [1, seq_len] i64
        //   Input:  context char ids  [1, ctx_len] i64
        //   Output: char logits       [1, vocab]   f32
        //
        // 推理流程:
        //   1. tokenize(pinyin) → input_ids
        //   2. encode(context)  → context_ids
        //   3. session.run(inputs![input_ids, context_ids]?)
        //   4. 从 logits 中提取 candidates 对应字的分数
        //   5. 按分数排序
        //
        // 示例代码 (ort v2):
        //
        // use ndarray::Array2;
        // let input = Array2::<i64>::zeros((1, pinyin_len));
        // let ctx   = Array2::<i64>::zeros((1, ctx_len));
        // let outputs = session.run(ort::inputs![input, ctx]?)?;
        // let logits = outputs[0].try_extract_tensor::<f32>()?;
        // ══════════════════════════════════════════════════

        // 占位分数：保持字典原始排序
        let n = candidates.len();
        Ok((0..n).map(|i| 1.0 - (i as f32 / n.max(1) as f32)).collect())
    }
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
        let ai = AIPredictor::new();
        // CI 环境下无 onnxruntime DLL 且无模型文件
        assert!(!ai.is_available());

        let history = HistoryBuffer::new(10);
        let cands = vec!["\u{662f}".into(), "\u{65f6}".into(), "\u{5341}".into()];
        let result = ai.rerank("shi", cands.clone(), &history);
        assert_eq!(result, cands);
    }
}
