//! # AI 推理引擎
//!
//! 基于 Duyu/Pinyin2Hanzi-Transformer 的 ONNX 推理引擎。
//!
//! ## 模型架构
//! - Encoder-Decoder Transformer (d=512, enc=8, dec=6)
//! - 输入: input_ids [1, 14] (拼音+声调), decoder_input_ids [1, 13] (自回归汉字)
//! - 输出: logits [1, 13, 23416] (汉字概率分布)
//! - 拼音格式: 带声调数字 (如 "ni3 hao3")
//!
//! ## 推理流程
//! 1. 编码拼音序列 → input_ids
//! 2. 自回归解码: <sos> → 逐字生成 → <eos>
//! 3. 每步取 logits 最后一个位置的 top-K

use std::path::{Path, PathBuf};

// ============================================================
// 上下文缓冲区
// ============================================================

pub struct HistoryBuffer {
    buf: Vec<String>,
    capacity: usize,
}

impl HistoryBuffer {
    pub fn new(capacity: usize) -> Self {
        Self { buf: Vec::with_capacity(capacity), capacity }
    }

    pub fn push(&mut self, text: &str) {
        if text.is_empty() { return; }
        if self.buf.len() >= self.capacity { self.buf.remove(0); }
        self.buf.push(text.to_string());
    }

    pub fn recent(&self, n: usize) -> Vec<&str> {
        let start = self.buf.len().saturating_sub(n);
        self.buf[start..].iter().map(|s| s.as_str()).collect()
    }

    pub fn context_string(&self) -> String { self.buf.join("") }
    pub fn is_empty(&self) -> bool { self.buf.is_empty() }
}

// ============================================================
// 词表索引
// ============================================================

pub struct VocabIndex {
    pub pinyin2id: std::collections::HashMap<String, i64>,
    pub char2id: std::collections::HashMap<String, i64>,
    pub id2char: std::collections::HashMap<i64, String>,
    pub max_pinyin_len: usize,   // 固定 14
    pub max_context_len: usize,  // 固定 13 (max_pinyin_len - 1)
    pub sos_id: i64,
    pub eos_id: i64,
    pub pad_id: i64,
}

impl VocabIndex {
    fn load_from_dir(dir: &Path) -> Option<Self> {
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
            let mpl = meta.get("max_pinyin_len").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
            (mpl, mpl - 1)
        } else {
            (14, 13)
        };

        // Special token IDs
        let sos_id = *pinyin2id.get("<sos>").unwrap_or(&2);
        let eos_id = *pinyin2id.get("<eos>").unwrap_or(&3);
        let pad_id = *pinyin2id.get("<pad>").unwrap_or(&0);

        eprintln!("[AI] vocab: {} pinyin, {} chars, src_len={}, tgt_len={}",
            pinyin2id.len(), char2id.len(), max_pinyin_len, max_context_len);
        Some(VocabIndex { pinyin2id, char2id, id2char, max_pinyin_len, max_context_len,
            sos_id, eos_id, pad_id })
    }
}

// ============================================================
// AI 状态
// ============================================================

pub enum AIState {
    Ready(ort::session::Session),
    Unavailable(String),
}

pub struct AIPredictor {
    state: AIState,
    vocab: Option<VocabIndex>,
    model_path: PathBuf,
    pub ai_first: bool,
}

impl AIPredictor {
    pub fn new() -> Self {
        match std::panic::catch_unwind(|| Self::try_init()) {
            Ok(predictor) => predictor,
            Err(_) => {
                eprintln!("[AI] ⚠ ort 初始化 panic, 回退字典模式");
                Self {
                    state: AIState::Unavailable("ort panic".into()),
                    vocab: None, model_path: PathBuf::new(), ai_first: false,
                }
            }
        }
    }

    fn try_init() -> Self {
        let model_path = find_model_path();
        let exe_dir = std::env::current_exe()
            .ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));

        // ORT_DYLIB_PATH
        if std::env::var("ORT_DYLIB_PATH").is_err() {
            if let Some(dir) = &exe_dir {
                let dll = dir.join("onnxruntime.dll");
                if dll.exists() {
                    eprintln!("[AI] ORT_DYLIB_PATH={:?}", dll);
                    std::env::set_var("ORT_DYLIB_PATH", &dll);
                }
            }
        }

        let vocab = exe_dir.as_ref().and_then(|d| VocabIndex::load_from_dir(d));

        let state = match &model_path {
            Some(path) => match load_model(path) {
                Ok(session) => {
                    eprintln!("[AI] ✅ model loaded: {:?}", path);
                    log_model_info(&session);
                    AIState::Ready(session)
                }
                Err(e) => {
                    eprintln!("[AI] ⚠ {}", e);
                    AIState::Unavailable(e)
                }
            },
            None => {
                eprintln!("[AI] ℹ weights.onnx not found, dict-only mode");
                AIState::Unavailable("weights.onnx not found".into())
            }
        };

        let ai_first = matches!(&state, AIState::Ready(_));
        Self { state, vocab, model_path: model_path.unwrap_or_default(), ai_first }
    }

    pub fn is_available(&self) -> bool {
        matches!(self.state, AIState::Ready(_)) && self.vocab.is_some()
    }

    pub fn model_path(&self) -> &Path { &self.model_path }

    /// AI 主导: 拼音 → 汉字短语 (自回归逐字解码)
    pub fn predict(
        &mut self,
        pinyin: &str,
        _context: &HistoryBuffer,
        top_k: usize,
    ) -> Vec<String> {
        let session = match &mut self.state {
            AIState::Ready(s) => s,
            _ => return vec![],
        };
        let vocab = match &self.vocab {
            Some(v) => v,
            None => return vec![],
        };

        match run_predict(session, vocab, pinyin, top_k) {
            Ok(candidates) => candidates,
            Err(e) => {
                eprintln!("[AI] predict error: {}", e);
                vec![]
            }
        }
    }

    /// 字典辅助: 候选词重排
    pub fn rerank(
        &mut self,
        pinyin: &str,
        candidates: Vec<String>,
        _context: &HistoryBuffer,
    ) -> Vec<String> {
        let session = match &mut self.state {
            AIState::Ready(s) => s,
            _ => return candidates,
        };
        let vocab = match &self.vocab {
            Some(v) => v,
            None => return candidates,
        };

        match run_rerank(session, vocab, pinyin, &candidates) {
            Ok(ranked) => ranked,
            Err(e) => {
                eprintln!("[AI] rerank error: {}", e);
                candidates
            }
        }
    }
}

// ============================================================
// 自回归预测
// ============================================================

/// 将无声调拼音转为带声调候选 (如 "ni" → 尝试 "ni1"~"ni4")
/// 返回这些key在词表中存在的ID, 如果都没有返回 <unk>
fn encode_syllable(syl: &str, vocab: &VocabIndex) -> i64 {
    // 先尝试直接查找 (可能已经带声调)
    if let Some(&id) = vocab.pinyin2id.get(syl) {
        return id;
    }
    // 无声调 → 尝试 1-4 声调
    for tone in 1..=4 {
        let with_tone = format!("{}{}", syl, tone);
        if let Some(&id) = vocab.pinyin2id.get(&with_tone) {
            return id;
        }
    }
    // 轻声
    let with_5 = format!("{}5", syl);
    if let Some(&id) = vocab.pinyin2id.get(&with_5) {
        return id;
    }
    1 // <unk>
}

/// 编码拼音序列为 input_ids [1, max_pinyin_len]
fn encode_pinyin(pinyin: &str, vocab: &VocabIndex) -> Vec<i64> {
    let syllables = crate::pinyin::split_pinyin_pub(pinyin);
    let mut ids = vec![vocab.pad_id; vocab.max_pinyin_len];

    ids[0] = vocab.sos_id;
    let mut pos = 1;
    for syl in syllables.iter().take(vocab.max_pinyin_len - 2) {
        ids[pos] = encode_syllable(syl, vocab);
        pos += 1;
    }
    if pos < vocab.max_pinyin_len {
        ids[pos] = vocab.eos_id;
    }

    ids
}

/// 运行一步推理: 给定 input_ids 和 decoder_input_ids, 返回 logits
fn run_one_step(
    session: &mut ort::session::Session,
    input_ids: &[i64],
    decoder_ids: &[i64],
    src_len: usize,
    tgt_len: usize,
) -> Result<Vec<f32>, String> {
    let src_value = ort::value::Tensor::from_array(
        ([1usize, src_len], input_ids.to_vec())
    ).map_err(|e| format!("src tensor: {}", e))?;

    let tgt_value = ort::value::Tensor::from_array(
        ([1usize, tgt_len], decoder_ids.to_vec())
    ).map_err(|e| format!("tgt tensor: {}", e))?;

    let outputs = session.run(ort::inputs![src_value, tgt_value])
        .map_err(|e| format!("session.run: {}", e))?;

    let logits_tensor = &outputs[0];
    let (_shape, logits_data) = logits_tensor
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("extract logits: {}", e))?;

    Ok(logits_data.to_vec())
}

/// 自回归预测: 拼音 → top-K 汉字短语
fn run_predict(
    session: &mut ort::session::Session,
    vocab: &VocabIndex,
    pinyin: &str,
    top_k: usize,
) -> Result<Vec<String>, String> {
    let syllables = crate::pinyin::split_pinyin_pub(pinyin);
    if syllables.is_empty() { return Ok(vec![]); }

    let input_ids = encode_pinyin(pinyin, vocab);
    let num_chars = syllables.len().min(vocab.max_context_len - 1); // 要生成多少个字

    // 贪心解码 (beam=top_k for first char, greedy after)
    let tgt_len = vocab.max_context_len;
    let hanzi_sos = *vocab.char2id.get("<sos>").unwrap_or(&2);
    let hanzi_eos = *vocab.char2id.get("<eos>").unwrap_or(&3);
    let hanzi_pad = *vocab.char2id.get("<pad>").unwrap_or(&0);

    // 第一步: 取 top-K 首字
    let mut decoder_ids = vec![hanzi_pad; tgt_len];
    decoder_ids[0] = hanzi_sos;

    let logits = run_one_step(session, &input_ids, &decoder_ids,
        vocab.max_pinyin_len, tgt_len)?;

    // logits 是 [1, tgt_len, vocab_size], 取位置 0 的 logits
    let vocab_size = vocab.id2char.len().max(vocab.char2id.len());
    let offset = 0 * vocab_size; // 第一个位置
    let first_logits = &logits[offset..offset + vocab_size];

    let mut scored: Vec<(i64, f32)> = first_logits.iter().enumerate()
        .filter(|(i, _)| *i >= 4) // 跳过 pad/unk/sos/eos
        .map(|(i, &s)| (i as i64, s))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let first_chars: Vec<(i64, String)> = scored.iter()
        .take(top_k)
        .filter_map(|(id, _)| vocab.id2char.get(id).map(|ch| (*id, ch.clone())))
        .collect();

    if num_chars == 1 {
        return Ok(first_chars.into_iter().map(|(_, ch)| ch).collect());
    }

    // 多字: 对每个首字做贪心续写
    let mut results = Vec::new();

    for (first_id, first_ch) in &first_chars {
        let mut phrase = first_ch.clone();
        let mut dec_ids = vec![hanzi_pad; tgt_len];
        dec_ids[0] = hanzi_sos;
        dec_ids[1] = *first_id;

        for step in 2..=num_chars {
            let logits = run_one_step(session, &input_ids, &dec_ids,
                vocab.max_pinyin_len, tgt_len)?;

            // 取位置 step-1 的 logits
            let offset = (step - 1) * vocab_size;
            if offset + vocab_size > logits.len() { break; }
            let step_logits = &logits[offset..offset + vocab_size];

            // 贪心: 取最高分
            let best_id = step_logits.iter().enumerate()
                .filter(|(i, _)| *i >= 4)
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i as i64)
                .unwrap_or(1);

            if best_id == hanzi_eos { break; }

            if let Some(ch) = vocab.id2char.get(&best_id) {
                phrase.push_str(ch);
            }

            if step < tgt_len {
                dec_ids[step] = best_id;
            }
        }

        results.push(phrase);
    }

    // 去重
    let mut seen = std::collections::HashSet::new();
    results.retain(|s| seen.insert(s.clone()));

    Ok(results)
}

/// 重排: 为每个候选词的首字取分数, 按分数排序
fn run_rerank(
    session: &mut ort::session::Session,
    vocab: &VocabIndex,
    pinyin: &str,
    candidates: &[String],
) -> Result<Vec<String>, String> {
    let input_ids = encode_pinyin(pinyin, vocab);
    let tgt_len = vocab.max_context_len;
    let hanzi_sos = *vocab.char2id.get("<sos>").unwrap_or(&2);
    let hanzi_pad = *vocab.char2id.get("<pad>").unwrap_or(&0);

    let mut decoder_ids = vec![hanzi_pad; tgt_len];
    decoder_ids[0] = hanzi_sos;

    let logits = run_one_step(session, &input_ids, &decoder_ids,
        vocab.max_pinyin_len, tgt_len)?;

    let vocab_size = vocab.char2id.len();
    let first_logits = &logits[..vocab_size.min(logits.len())];

    let scores: Vec<f32> = candidates.iter().map(|cand| {
        if let Some(first_char) = cand.chars().next() {
            if let Some(&char_id) = vocab.char2id.get(&first_char.to_string()) {
                let idx = char_id as usize;
                if idx < first_logits.len() {
                    return first_logits[idx];
                }
            }
        }
        f32::NEG_INFINITY
    }).collect();

    let mut indexed: Vec<(usize, f32)> = scores.into_iter().enumerate().collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    Ok(indexed.into_iter()
        .filter_map(|(i, _)| candidates.get(i).cloned())
        .collect())
}

// ============================================================
// 辅助函数
// ============================================================

fn find_model_path() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
    if let Some(dir) = &exe_dir {
        let p = dir.join("weights.onnx");
        if p.exists() { return Some(p); }
    }
    let p = PathBuf::from("weights.onnx");
    if p.exists() { Some(p) } else { None }
}

fn load_model(path: &Path) -> Result<ort::session::Session, String> {
    eprintln!("[AI] loading {:?} ...", path);
    let start = std::time::Instant::now();
    let session = ort::session::Session::builder()
        .map_err(|e| format!("builder: {}", e))?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
        .map_err(|e| format!("opt: {}", e))?
        .with_intra_threads(2)
        .map_err(|e| format!("threads: {}", e))?
        .commit_from_file(path)
        .map_err(|e| format!("load: {}", e))?;
    eprintln!("[AI] loaded in {:?}", start.elapsed());
    Ok(session)
}

fn log_model_info(session: &ort::session::Session) {
    eprintln!("[AI] inputs: {}, outputs: {}",
        session.inputs().len(), session.outputs().len());
    for inp in session.inputs() {
        eprintln!("[AI]   in: {}", inp.name());
    }
    for out in session.outputs() {
        eprintln!("[AI]   out: {}", out.name());
    }
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
        h.push("\u{754c}");
        assert_eq!(h.context_string(), "\u{597d}\u{4e16}\u{754c}");
    }

    #[test]
    fn test_ai_fallback() {
        let mut ai = AIPredictor::new();
        assert!(!ai.is_available());
        let history = HistoryBuffer::new(10);
        let cands = vec!["\u{662f}".into(), "\u{65f6}".into(), "\u{5341}".into()];
        let result = ai.rerank("shi", cands.clone(), &history);
        assert_eq!(result, cands);
    }
}
