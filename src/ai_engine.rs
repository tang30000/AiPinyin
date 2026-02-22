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
    /// 汉字 → 拼音 (反向映射, 用于构建 Concat 上下文)
    pub char2pinyin: HashMap<String, String>,
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

        if !ch_path.exists() {
            eprintln!("[AI] char2id.json not found in {:?}", dir);
            return None;
        }

        let ch_text = std::fs::read_to_string(&ch_path).ok()?;

        // pinyin2id 可选 (GPT2-Chinese 不需要)
        let pinyin2id: HashMap<String, i64> = if py_path.exists() {
            let py_text = std::fs::read_to_string(&py_path).ok()?;
            serde_json::from_str(&py_text).ok()?
        } else {
            eprintln!("[AI] pinyin2id.json 不存在 (GPT2-Chinese 模式)");
            HashMap::new()
        };
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

        // 构建 char → pinyin 反向映射 (取第一个匹配的拼音)
        let mut char2pinyin = HashMap::new();
        for (py, chars) in &pinyin2char {
            for ch in chars {
                char2pinyin.entry(ch.clone()).or_insert_with(|| py.clone());
            }
        }

        let cls_id = *char2id.get("<sos>").unwrap_or(&101);
        let sep_id = *char2id.get("<eos>").unwrap_or(&102);
        let pad_id = *char2id.get("<pad>").unwrap_or(&0);

        eprintln!("[AI] vocab: {} pinyin, {} chars, {} pinyin2char, {} char2pinyin",
            pinyin2id.len(), char2id.len(), pinyin2char_ids.len(), char2pinyin.len());
        Some(VocabIndex {
            pinyin2id, char2id, id2char, pinyin2char, pinyin2char_ids, char2pinyin,
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

    /// AI 主导: 字典引导的上下文感知预测
    pub fn predict(
        &mut self, pinyin: &str, context: &HistoryBuffer, top_k: usize,
        dict_words: &[String],
    ) -> Vec<String> {
        let session = match &mut self.state {
            AIState::Ready(s) => s, _ => return vec![],
        };
        let vocab = match &self.vocab {
            Some(v) => v, None => return vec![],
        };
        let ctx_str = context.context_string();
        match run_predict(session, vocab, pinyin, top_k, &ctx_str, dict_words) {
            Ok(c) => c,
            Err(e) => { eprintln!("[AI] predict: {}", e); vec![] }
        }
    }

    /// 字典辅助: 上下文感知重排
    pub fn rerank(
        &mut self, pinyin: &str, candidates: Vec<String>, context: &HistoryBuffer,
    ) -> Vec<String> {
        let session = match &mut self.state {
            AIState::Ready(s) => s, _ => return candidates,
        };
        let vocab = match &self.vocab {
            Some(v) => v, None => return candidates,
        };
        let ctx_str = context.context_string();
        match run_rerank(session, vocab, pinyin, &candidates, &ctx_str) {
            Ok(r) => r,
            Err(e) => { eprintln!("[AI] rerank: {}", e); candidates }
        }
    }
}

// ============================================================
// ONNX 推理
// ============================================================

/// 运行推理: input_ids → logits (GPT2-Chinese 只需要 input_ids)
fn run_inference(
    session: &mut ort::session::Session,
    input_ids: &[i64],
) -> Result<Vec<f32>, String> {
    let seq_len = input_ids.len();

    let ids_tensor = ort::value::Tensor::from_array(
        ([1usize, seq_len], input_ids.to_vec())
    ).map_err(|e| format!("ids tensor: {}", e))?;

    let outputs = session.run(ort::inputs![ids_tensor])
        .map_err(|e| format!("session.run: {}", e))?;

    let (_shape, logits) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("extract: {}", e))?;

    Ok(logits.to_vec())
}

/// 构建上下文前缀: [CLS] char1 char2 ... (纯字符序列)
///
/// GPT2-Chinese 接受纯字符输入, 不需要拼音 token
fn build_context(vocab: &VocabIndex, context: &str) -> Vec<i64> {
    let mut ids = vec![vocab.cls_id];
    let ctx_chars: Vec<char> = context.chars().rev().take(50).collect::<Vec<_>>()
        .into_iter().rev().collect();
    
    for ch in &ctx_chars {
        let ch_str = ch.to_string();
        if let Some(&ch_id) = vocab.char2id.get(&ch_str) {
            ids.push(ch_id);
        }
    }
    ids
}

/// 字典引导评分 (GPT2-Chinese: 纯字符, 无拼音 token)
///
/// 上下文 = [CLS] char1 char2 ... → 预测下一个字, 用拼音约束选字
fn run_predict(
    session: &mut ort::session::Session,
    vocab: &VocabIndex,
    pinyin: &str,
    top_k: usize,
    context: &str,
    dict_words: &[String],
) -> Result<Vec<String>, String> {
    let syllables = crate::pinyin::split_pinyin_pub(pinyin);
    if syllables.is_empty() { return Ok(vec![]); }

    let vocab_size = 21128usize;
    let ctx_prefix = build_context(vocab, context);
    let ctx_len = ctx_prefix.len() - 1;

    if ctx_len > 0 {
        eprintln!("[AI] predict: ctx={}字, pinyin={}, dict_words={}",
            ctx_len, pinyin, dict_words.len());
    }

    // === 单音节: 直接约束解码 ===
    if syllables.len() == 1 {
        let logits = run_inference(session, &ctx_prefix)?;
        let offset = (ctx_prefix.len() - 1) * vocab_size;
        if offset + vocab_size > logits.len() { return Err("logits too short".into()); }
        let chars = get_top_k_constrained(&logits[offset..offset + vocab_size], vocab, &syllables[0], top_k);
        return Ok(chars.into_iter().map(|(_, ch)| ch).collect());
    }

    // === 3+音节: 词图分词 → AI 评分 ===
    if syllables.len() >= 3 {
        let graph_cands = word_graph_segment(&syllables, 5);
        if !graph_cands.is_empty() {
            eprintln!("[AI] 词图分词: {} 条", graph_cands.len());
            for (i, c) in graph_cands.iter().enumerate().take(3) {
                eprintln!("[AI]   #{}: {}", i+1, c);
            }
            // 用 AI 对词图候选评分 (前4字累计概率, 够区分)
            let mut scored: Vec<(String, f32)> = Vec::new();
            for sentence in &graph_cands {
                let chars: Vec<char> = sentence.chars().collect();
                let score_len = std::cmp::min(4, chars.len());
                let mut ids = ctx_prefix.clone();
                let mut total_score = 0.0f32;
                let mut valid = true;
                for ch in &chars[..score_len] {
                    let ch_str = ch.to_string();
                    let ch_id = match vocab.char2id.get(&ch_str) {
                        Some(&id) => id,
                        None => { valid = false; break; }
                    };
                    let logits = run_inference(session, &ids)?;
                    let offset = (ids.len() - 1) * vocab_size;
                    if offset + ch_id as usize >= logits.len() { valid = false; break; }
                    total_score += logits[offset + ch_id as usize];
                    ids.push(ch_id);
                }
                if valid {
                    scored.push((sentence.clone(), total_score));
                }
            }
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            if !scored.is_empty() {
                let result: Vec<String> = scored.into_iter().take(top_k).map(|(s, _)| s).collect();
                return Ok(result);
            }
        }
    }

    // === 2音节: 字典引导评分 ===
    let target_len = syllables.len();
    let candidates: Vec<&String> = dict_words.iter()
        .filter(|w| w.chars().count() == target_len)
        .take(9)
        .collect();

    if candidates.is_empty() {
        return run_predict_greedy(session, vocab, &syllables, &ctx_prefix, vocab_size, top_k);
    }

    // 首字分数 (1 次推理)
    let logits1 = run_inference(session, &ctx_prefix)?;
    let offset1 = (ctx_prefix.len() - 1) * vocab_size;

    let mut by_first: std::collections::HashMap<String, Vec<&String>> = std::collections::HashMap::new();
    for word in &candidates {
        if let Some(ch) = word.chars().next() {
            by_first.entry(ch.to_string()).or_default().push(word);
        }
    }

    let mut scored: Vec<(String, f32)> = Vec::new();

    for (first_ch_str, words) in &by_first {
        let first_id = match vocab.char2id.get(first_ch_str) {
            Some(&id) => id, None => continue,
        };
        let first_score = if offset1 + first_id as usize >= logits1.len() { -50.0 }
                          else { logits1[offset1 + first_id as usize] };

        if target_len == 2 {
            // 二字词: [ctx][char1] → 预测 char2
            let mut ids2 = ctx_prefix.clone();
            ids2.push(first_id);
            let logits2 = run_inference(session, &ids2)?;
            let offset2 = (ids2.len() - 1) * vocab_size;

            for word in words {
                let chars: Vec<char> = word.chars().collect();
                if chars.len() != 2 { continue; }
                let ch2_str = chars[1].to_string();
                let ch2_score = vocab.char2id.get(&ch2_str)
                    .map(|&cid| {
                        let p = offset2 + cid as usize;
                        if p < logits2.len() { logits2[p] } else { -50.0 }
                    })
                    .unwrap_or(-50.0);
                scored.push((word.to_string(), first_score + ch2_score));
            }
        } else {
            for word in words {
                let chars: Vec<char> = word.chars().collect();
                let mut total = first_score;
                let mut cur_ids = ctx_prefix.clone();
                cur_ids.push(first_id);
                for i in 1..target_len {
                    let logits = run_inference(session, &cur_ids)?;
                    let lp = (cur_ids.len() - 1) * vocab_size;
                    let ch_str = chars[i].to_string();
                    let ch_score = vocab.char2id.get(&ch_str)
                        .and_then(|&cid| { let p = lp + cid as usize; if p < logits.len() { Some(logits[p]) } else { None } })
                        .unwrap_or(-50.0);
                    total += ch_score;
                    cur_ids.push(vocab.char2id.get(&ch_str).copied().unwrap_or(vocab.unk_id));
                }
                scored.push((word.to_string(), total));
            }
        }
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let result: Vec<String> = scored.into_iter().take(top_k).map(|(w, _)| w).collect();

    if result.len() < top_k {
        let mut r = result;
        if let Ok(greedy) = run_predict_greedy(session, vocab, &syllables, &ctx_prefix, vocab_size, top_k) {
            for g in greedy {
                if !r.contains(&g) { r.push(g); }
                if r.len() >= top_k { break; }
            }
        }
        Ok(r)
    } else {
        Ok(result)
    }
}

/// 贪心生成 (GPT2-Chinese: 纯字符自回归)
fn run_predict_greedy(
    session: &mut ort::session::Session,
    vocab: &VocabIndex,
    syllables: &[String],
    ctx_prefix: &[i64],
    vocab_size: usize,
    top_k: usize,
) -> Result<Vec<String>, String> {
    let py1 = &syllables[0];
    let logits = run_inference(session, ctx_prefix)?;
    let offset = (ctx_prefix.len() - 1) * vocab_size;
    if offset + vocab_size > logits.len() { return Err("logits too short".into()); }
    let first_chars = get_top_k_constrained(&logits[offset..offset + vocab_size], vocab, py1, top_k);
    if first_chars.is_empty() { return Ok(vec![]); }

    let mut results = Vec::new();
    for (first_id, first_ch) in &first_chars {
        let mut phrase = first_ch.clone();
        let mut current_ids = ctx_prefix.to_vec();
        current_ids.push(*first_id);

        for syl in syllables.iter().skip(1) {
            let logits = run_inference(session, &current_ids)?;
            let lp = (current_ids.len() - 1) * vocab_size;
            if lp + vocab_size > logits.len() { break; }

            let top1 = get_top_k_constrained(&logits[lp..lp + vocab_size], vocab, syl, 1);
            if let Some((char_id, ch)) = top1.into_iter().next() {
                phrase.push_str(&ch);
                current_ids.push(char_id);
            } else {
                break;
            }
        }
        results.push(phrase);
    }

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

/// 上下文感知重排
///
/// 实验验证 (test_rerank.py):
///   无上下文: wenti → 文体=7.1 > 问题=5.2  (AI 排错)
///   上下文"我想问你一个": wenti → 问题=8.0 > 文体=6.9  (AI 排对!)
///
/// 策略: 上下文 + AI 评分首字
fn run_rerank(
    session: &mut ort::session::Session,
    vocab: &VocabIndex,
    pinyin: &str,
    candidates: &[String],
    context: &str,
) -> Result<Vec<String>, String> {
    let syllables = crate::pinyin::split_pinyin_pub(pinyin);
    if syllables.is_empty() || candidates.is_empty() {
        return Ok(candidates.to_vec());
    }

    let vocab_size = 21128usize;
    let n = candidates.len();

    // 构建纯字符上下文
    let input_ids = build_context(vocab, context);
    let ctx_len = input_ids.len() - 1;

    // 直接推理 (不加拼音 token)
    let logits = run_inference(session, &input_ids)?;
    let offset = (input_ids.len() - 1) * vocab_size;

    // 提取每个候选首字的 AI 分数
    let ai_scores: Vec<f32> = candidates.iter().map(|cand| {
        cand.chars().next()
            .and_then(|ch| vocab.char2id.get(&ch.to_string()))
            .and_then(|&cid| {
                let pos = offset + cid as usize;
                if pos < logits.len() { Some(logits[pos]) } else { None }
            })
            .unwrap_or(-50.0)
    }).collect();

    // === 动态 AI 权重 ===
    // 上下文越长 → AI 越可信 → AI 权重越高
    let ai_weight = if ctx_len == 0 {
        15.0   // 无上下文: AI 基本不干预
    } else if ctx_len <= 2 {
        35.0   // 短上下文: AI 适度参与
    } else if ctx_len <= 4 {
        55.0   // 中上下文: AI 与字典各半
    } else {
        70.0   // 长上下文: AI 主导
    };
    let dict_weight = 100.0 - ai_weight;

    if ctx_len > 0 {
        eprintln!("[AI] rerank: ctx={}字 '...{}', ai_weight={:.0}%",
            ctx_len, &context[context.len().saturating_sub(12)..], ai_weight);
    }

    // === 混合评分 ===
    let ai_min = ai_scores.iter().cloned().fold(f32::INFINITY, f32::min);
    let ai_max = ai_scores.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let ai_range = (ai_max - ai_min).max(0.1);

    let mut scored: Vec<(usize, f32)> = Vec::with_capacity(n);

    for (idx, cand) in candidates.iter().enumerate() {
        let char_count = cand.chars().count();

        // 字典位序分 (归一化到 0~100)
        let dict_norm = 100.0 - (idx as f32) * (100.0 / n.max(1) as f32);

        // AI 归一化分 (0~100)
        let ai_norm = (ai_scores[idx] - ai_min) / ai_range * 100.0;

        // 词长匹配加分
        let len_bonus = if char_count == syllables.len() && char_count >= 2 {
            20.0  // 完整词组匹配
        } else if char_count == syllables.len() {
            5.0
        } else {
            0.0
        };

        let final_score = dict_norm * dict_weight / 100.0
                        + ai_norm * ai_weight / 100.0
                        + len_bonus;
        scored.push((idx, final_score));
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    Ok(scored.into_iter()
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

// ============================================================
// 词图分词 — 长输入拆分为字典词组
// ============================================================

/// 词图最短路径分词: 将音节序列拆分为字典中的词组
///
/// 例: ["zhi","chi","de","zen","me","yang"] 
///   → "支持的怎么样" (匹配 支持 + 的 + 怎么 + 样)
///
/// 返回 top-K 完整句子候选 (按词频权重排序)
pub fn word_graph_segment(syllables: &[String], top_k: usize) -> Vec<String> {
    let n = syllables.len();
    if n == 0 { return vec![]; }

    // 获取全局字典
    let dict = match crate::pinyin::get_dict() {
        Some(d) => d,
        None => return vec![],
    };

    // DP: best[i] = Vec<(score, path)> 从位置 i 到末尾的最佳分词
    // 保留 top-K 条路径
    let mut best: Vec<Option<Vec<(i64, Vec<String>)>>> = vec![None; n + 1];
    best[n] = Some(vec![(0, vec![])]);

    for i in (0..n).rev() {
        let mut candidates: Vec<(i64, Vec<String>)> = Vec::new();

        // 尝试匹配长度 1~min(6, n-i) 的词组
        for length in 1..=std::cmp::min(6, n - i) {
            let j = i + length;
            let rest = match &best[j] {
                Some(paths) => paths,
                None => continue,
            };

            // 拼接这几个音节
            let py_key: String = syllables[i..j].concat();
            let entries = dict.lookup(&py_key);
            if entries.is_empty() { continue; }

            // 取权重最高的 top-3 词 (不只 top-1, 让 AI 做最终选择)
            let mut sorted_entries: Vec<&crate::pinyin::Candidate> = entries.iter().collect();
            sorted_entries.sort_by(|a, b| b.weight.cmp(&a.weight));
            let top_words: Vec<_> = sorted_entries.iter().take(3).collect();

            let length_bonus = (length as i64) * 200;

            for entry in &top_words {
                let word = &entry.word;
                let score = entry.weight as i64 + length_bonus;

                for (rest_score, rest_path) in rest.iter().take(2) {
                    let total = score + rest_score;
                    let mut path = vec![word.clone()];
                    path.extend_from_slice(rest_path);
                    candidates.push((total, path));
                }
            }
        }

        if !candidates.is_empty() {
            // 按分数降序排序, 保留 top-K
            candidates.sort_by(|a, b| b.0.cmp(&a.0));
            candidates.dedup_by(|a, b| {
                let sa: String = a.1.concat();
                let sb: String = b.1.concat();
                sa == sb
            });
            candidates.truncate(8);
            best[i] = Some(candidates);
        }
    }

    match &best[0] {
        Some(paths) => {
            paths.iter()
                .map(|(_, words)| words.concat())
                .collect()
        }
        None => vec![],
    }
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
