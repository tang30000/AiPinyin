//! # AI 推理引擎 — PinyinGPT (Concat 模式)
//!
//! 基于 aihijo/transformers4ime-pinyingpt-concat 的 ONNX 推理。
//!
//! ## 模型架构
//! - GPT-2 (12层, 12头, 768维, 102.4M 参数)
//! - 输入: input_ids [1, seq], attention_mask [1, seq], position_ids [1, seq]
//! - 输出: logits [1, seq, 21571]
//!
//! ## Concat 模式
//! - 拼音和汉字交替拼接: [CLS] [py1] char1 [py2] char2 ... [SEP]
//! - 拼音用特殊 token 表示: [ni]=21128, [hao]=21129, ...
//! - 每输入一个拼音 token, 取 logits 最后位置预测对应汉字
//! - pinyin2char.json 约束候选: 只在该拼音对应的汉字中选择

use std::path::{Path, PathBuf};
use std::collections::HashMap;

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
    /// 无声调拼音 → token ID: "ni" → 21128
    pub pinyin2id: HashMap<String, i64>,
    /// 汉字 → token ID
    pub char2id: HashMap<String, i64>,
    /// token ID → 汉字
    pub id2char: HashMap<i64, String>,
    /// 拼音 → 候选汉字列表: "ni" → ["你","尼","泥",...]
    pub pinyin2char: HashMap<String, Vec<String>>,
    /// 拼音 → 候选汉字 token IDs (预计算)
    pub pinyin2char_ids: HashMap<String, Vec<i64>>,
    /// 特殊 token IDs
    pub cls_id: i64,  // [CLS] = 101
    pub sep_id: i64,  // [SEP] = 102
    pub pad_id: i64,  // [PAD] = 0
    pub unk_id: i64,  // [UNK] = 100
}

impl VocabIndex {
    fn load_from_dir(dir: &Path) -> Option<Self> {
        let py_path = dir.join("pinyin2id.json");
        let ch_path = dir.join("char2id.json");
        let p2c_path = dir.join("pinyin2char.json");

        if !py_path.exists() || !ch_path.exists() {
            eprintln!("[AI] vocab files not found in {:?}", dir);
            return None;
        }

        let py_text = std::fs::read_to_string(&py_path).ok()?;
        let ch_text = std::fs::read_to_string(&ch_path).ok()?;

        let pinyin2id: HashMap<String, i64> = serde_json::from_str(&py_text).ok()?;
        let char2id: HashMap<String, i64> = serde_json::from_str(&ch_text).ok()?;
        let id2char: HashMap<i64, String> = char2id.iter().map(|(k, v)| (*v, k.clone())).collect();

        // 加载 pinyin2char 映射
        let pinyin2char: HashMap<String, Vec<String>> = if p2c_path.exists() {
            let p2c_text = std::fs::read_to_string(&p2c_path).ok()?;
            serde_json::from_str(&p2c_text).ok()?
        } else {
            HashMap::new()
        };

        // 预计算 pinyin → candidate IDs
        let unk_id = *char2id.get("<unk>").unwrap_or(&100);
        let mut pinyin2char_ids = HashMap::new();
        for (py, chars) in &pinyin2char {
            let ids: Vec<i64> = chars.iter()
                .filter_map(|ch| char2id.get(ch).copied())
                .filter(|&id| id != unk_id)
                .collect();
            if !ids.is_empty() {
                pinyin2char_ids.insert(py.clone(), ids);
            }
        }

        let cls_id = *char2id.get("<sos>").unwrap_or(&101);
        let sep_id = *char2id.get("<eos>").unwrap_or(&102);
        let pad_id = *char2id.get("<pad>").unwrap_or(&0);

        eprintln!("[AI] vocab: {} pinyin, {} chars, {} pinyin2char",
            pinyin2id.len(), char2id.len(), pinyin2char_ids.len());
        Some(VocabIndex {
            pinyin2id, char2id, id2char, pinyin2char, pinyin2char_ids,
            cls_id, sep_id, pad_id, unk_id,
        })
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
            Ok(p) => p,
            Err(_) => {
                eprintln!("[AI] ⚠ ort panic, 回退字典模式");
                Self { state: AIState::Unavailable("ort panic".into()),
                    vocab: None, model_path: PathBuf::new(), ai_first: false }
            }
        }
    }

    fn try_init() -> Self {
        let model_path = find_model_path();
        let exe_dir = std::env::current_exe()
            .ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));

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
                    eprintln!("[AI] ✅ PinyinGPT loaded: {:?}", path);
                    log_model_info(&session);
                    AIState::Ready(session)
                }
                Err(e) => { eprintln!("[AI] ⚠ {}", e); AIState::Unavailable(e) }
            },
            None => {
                eprintln!("[AI] ℹ weights.onnx not found, dict-only");
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

    /// AI 主导: 拼音 → 汉字 (Concat 自回归)
    pub fn predict(
        &mut self, pinyin: &str, _context: &HistoryBuffer, top_k: usize,
    ) -> Vec<String> {
        let session = match &mut self.state {
            AIState::Ready(s) => s, _ => return vec![],
        };
        let vocab = match &self.vocab {
            Some(v) => v, None => return vec![],
        };
        match run_predict(session, vocab, pinyin, top_k) {
            Ok(c) => c,
            Err(e) => { eprintln!("[AI] predict: {}", e); vec![] }
        }
    }

    /// 字典辅助: 候选重排 (用第一个拼音的 logits 给候选打分)
    pub fn rerank(
        &mut self, pinyin: &str, candidates: Vec<String>, _context: &HistoryBuffer,
    ) -> Vec<String> {
        let session = match &mut self.state {
            AIState::Ready(s) => s, _ => return candidates,
        };
        let vocab = match &self.vocab {
            Some(v) => v, None => return candidates,
        };
        match run_rerank(session, vocab, pinyin, &candidates) {
            Ok(r) => r,
            Err(e) => { eprintln!("[AI] rerank: {}", e); candidates }
        }
    }
}

// ============================================================
// ONNX 推理
// ============================================================

/// 运行推理: input_ids + attention_mask + position_ids → logits
fn run_inference(
    session: &mut ort::session::Session,
    input_ids: &[i64],
) -> Result<Vec<f32>, String> {
    let seq_len = input_ids.len();

    let attention_mask: Vec<i64> = input_ids.iter()
        .map(|&id| if id != 0 { 1 } else { 0 }).collect();

    let position_ids: Vec<i64> = (0..seq_len as i64).collect();

    let ids_tensor = ort::value::Tensor::from_array(
        ([1usize, seq_len], input_ids.to_vec())
    ).map_err(|e| format!("ids tensor: {}", e))?;

    let mask_tensor = ort::value::Tensor::from_array(
        ([1usize, seq_len], attention_mask)
    ).map_err(|e| format!("mask tensor: {}", e))?;

    let pos_tensor = ort::value::Tensor::from_array(
        ([1usize, seq_len], position_ids)
    ).map_err(|e| format!("pos tensor: {}", e))?;

    let outputs = session.run(ort::inputs![ids_tensor, mask_tensor, pos_tensor])
        .map_err(|e| format!("session.run: {}", e))?;

    let (_shape, logits) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("extract: {}", e))?;

    Ok(logits.to_vec())
}

/// Concat 自回归预测: 拼音 → top-K 汉字短语
fn run_predict(
    session: &mut ort::session::Session,
    vocab: &VocabIndex,
    pinyin: &str,
    top_k: usize,
) -> Result<Vec<String>, String> {
    let syllables = crate::pinyin::split_pinyin_pub(pinyin);
    if syllables.is_empty() { return Ok(vec![]); }

    // Concat 格式: [CLS] [py1] → 预测char1, [CLS] [py1] char1 [py2] → 预测char2, ...

    // 第一步: [CLS] [py1] → top-K 首字
    let py1 = &syllables[0];
    let py1_id = match vocab.pinyin2id.get(py1.as_str()) {
        Some(&id) => id,
        None => return Ok(vec![]),  // 未知拼音
    };

    let input_ids = vec![vocab.cls_id, py1_id];
    let logits = run_inference(session, &input_ids)?;

    // 取最后位置的 logits
    let vocab_size = 21571usize;
    let last_pos = input_ids.len() - 1;
    let offset = last_pos * vocab_size;
    if offset + vocab_size > logits.len() {
        return Err("logits too short".into());
    }
    let last_logits = &logits[offset..offset + vocab_size];

    // 拼音约束: 只在 py1 对应的候选汉字中选
    let first_chars = get_top_k_constrained(last_logits, vocab, py1, top_k);
    if first_chars.is_empty() { return Ok(vec![]); }

    if syllables.len() == 1 {
        return Ok(first_chars.into_iter().map(|(_, ch)| ch).collect());
    }

    // 多音节: 对每个首字做贪心续写
    let mut results = Vec::new();
    for (first_id, first_ch) in &first_chars {
        let mut phrase = first_ch.clone();
        let mut current_ids = vec![vocab.cls_id, py1_id, *first_id];

        for syl in syllables.iter().skip(1) {
            let py_id = match vocab.pinyin2id.get(syl.as_str()) {
                Some(&id) => id,
                None => break,
            };
            current_ids.push(py_id);

            let logits = run_inference(session, &current_ids)?;
            let last_pos = current_ids.len() - 1;
            let offset = last_pos * vocab_size;
            if offset + vocab_size > logits.len() { break; }
            let step_logits = &logits[offset..offset + vocab_size];

            // 贪心: 拼音约束 top-1
            let top1 = get_top_k_constrained(step_logits, vocab, syl, 1);
            if let Some((char_id, ch)) = top1.into_iter().next() {
                phrase.push_str(&ch);
                current_ids.push(char_id);
            } else {
                break;
            }
        }
        results.push(phrase);
    }

    // 去重
    let mut seen = std::collections::HashSet::new();
    results.retain(|s| seen.insert(s.clone()));
    Ok(results)
}

/// 拼音约束的 top-K 选取
fn get_top_k_constrained(
    logits: &[f32],
    vocab: &VocabIndex,
    pinyin: &str,
    top_k: usize,
) -> Vec<(i64, String)> {
    if let Some(candidate_ids) = vocab.pinyin2char_ids.get(pinyin) {
        // 在候选中选 top-K
        let mut scored: Vec<(i64, f32)> = candidate_ids.iter()
            .filter_map(|&id| {
                let idx = id as usize;
                if idx < logits.len() { Some((id, logits[idx])) } else { None }
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scored.iter().take(top_k)
            .filter_map(|(id, _)| vocab.id2char.get(id).map(|ch| (*id, ch.clone())))
            .collect()
    } else {
        // 无约束 fallback
        let mut scored: Vec<(i64, f32)> = logits.iter().enumerate()
            .filter(|(i, _)| *i >= 4)
            .map(|(i, &s)| (i as i64, s))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scored.iter().take(top_k)
            .filter_map(|(id, _)| vocab.id2char.get(id).map(|ch| (*id, ch.clone())))
            .collect()
    }
}

/// 重排
fn run_rerank(
    session: &mut ort::session::Session,
    vocab: &VocabIndex,
    pinyin: &str,
    candidates: &[String],
) -> Result<Vec<String>, String> {
    let syllables = crate::pinyin::split_pinyin_pub(pinyin);
    if syllables.is_empty() { return Ok(candidates.to_vec()); }

    let py1 = &syllables[0];
    let py1_id = match vocab.pinyin2id.get(py1.as_str()) {
        Some(&id) => id,
        None => return Ok(candidates.to_vec()),
    };

    let input_ids = vec![vocab.cls_id, py1_id];
    let logits = run_inference(session, &input_ids)?;

    let vocab_size = 21571usize;
    let last_pos = input_ids.len() - 1;
    let offset = last_pos * vocab_size;
    let last_logits = &logits[offset..offset.saturating_add(vocab_size).min(logits.len())];

    let scores: Vec<f32> = candidates.iter().map(|cand| {
        if let Some(first_char) = cand.chars().next() {
            if let Some(&char_id) = vocab.char2id.get(&first_char.to_string()) {
                let idx = char_id as usize;
                if idx < last_logits.len() { return last_logits[idx]; }
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
// 辅助
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
    for inp in session.inputs() { eprintln!("[AI]   in: {}", inp.name()); }
    for out in session.outputs() { eprintln!("[AI]   out: {}", out.name()); }
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
        h.push("\u{4f60}"); h.push("\u{597d}"); h.push("\u{4e16}");
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
