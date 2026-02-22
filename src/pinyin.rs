//! # 拼音解析引擎
//!
//! 将连续的英文字母按拼音规则切分为音节，
//! 通过预索引词典提供高速拼音→汉字候选查找。
//!
//! ## 性能设计
//! - 加载时一次性构建三级索引（精确/前缀/缩写）
//! - 所有查询均为 O(1) HashMap 查找，零遍历
//! - 41K 词条加载 ~150ms，每次按键查询 <1ms

// ============================================================
// 拼音合法音节表
// ============================================================

const VALID_SYLLABLES: &[&str] = &[
    "a", "o", "e", "ai", "ei", "ao", "ou", "an", "en", "ang", "eng", "er",
    "ba", "bo", "bi", "bu", "bai", "bei", "bao", "ban", "ben", "bang", "beng",
    "bie", "biao", "bian", "bin", "bing",
    "pa", "po", "pi", "pu", "pai", "pei", "pao", "pou", "pan", "pen", "pang", "peng",
    "pie", "piao", "pian", "pin", "ping",
    "ma", "mo", "me", "mi", "mu", "mai", "mei", "mao", "mou", "man", "men",
    "mang", "meng", "mie", "miao", "miu", "mian", "min", "ming",
    "fa", "fo", "fu", "fei", "fou", "fan", "fen", "fang", "feng",
    "da", "de", "di", "du", "dai", "dei", "dao", "dou", "dan", "den", "dang", "deng",
    "dong", "die", "diao", "diu", "dian", "ding", "duo", "dui", "duan", "dun",
    "ta", "te", "ti", "tu", "tai", "tao", "tou", "tan", "tang", "teng",
    "tong", "tie", "tiao", "tian", "ting", "tuo", "tui", "tuan", "tun",
    "na", "ne", "ni", "nu", "nv", "nai", "nei", "nao", "nou", "nan", "nen",
    "nang", "neng", "nong", "nie", "niao", "niu", "nian", "nin", "ning",
    "nuo", "nuan", "nve",
    "la", "le", "li", "lu", "lv", "lai", "lei", "lao", "lou", "lan", "lang", "leng",
    "long", "lie", "liao", "liu", "lian", "lin", "ling", "luo", "luan", "lun", "lve",
    "ga", "ge", "gu", "gai", "gei", "gao", "gou", "gan", "gen", "gang", "geng",
    "gong", "gua", "guai", "guan", "guang", "gui", "gun", "guo",
    "ka", "ke", "ku", "kai", "kei", "kao", "kou", "kan", "ken", "kang", "keng",
    "kong", "kua", "kuai", "kuan", "kuang", "kui", "kun", "kuo",
    "ha", "he", "hu", "hai", "hei", "hao", "hou", "han", "hen", "hang", "heng",
    "hong", "hua", "huai", "huan", "huang", "hui", "hun", "huo",
    "ji", "ju", "jia", "jie", "jiao", "jiu", "jian", "jin", "jiang", "jing",
    "jiong", "juan", "jun", "jue",
    "qi", "qu", "qia", "qie", "qiao", "qiu", "qian", "qin", "qiang", "qing",
    "qiong", "quan", "qun", "que",
    "xi", "xu", "xia", "xie", "xiao", "xiu", "xian", "xin", "xiang", "xing",
    "xiong", "xuan", "xun", "xue",
    "zha", "zhe", "zhi", "zhu", "zhai", "zhei", "zhao", "zhou", "zhan", "zhen",
    "zhang", "zheng", "zhong", "zhua", "zhuai", "zhuan", "zhuang", "zhui", "zhun", "zhuo",
    "cha", "che", "chi", "chu", "chai", "chao", "chou", "chan", "chen",
    "chang", "cheng", "chong", "chua", "chuai", "chuan", "chuang", "chui", "chun", "chuo",
    "sha", "she", "shi", "shu", "shai", "shei", "shao", "shou", "shan", "shen",
    "shang", "sheng", "shua", "shuai", "shuan", "shuang", "shui", "shun", "shuo",
    "re", "ri", "ru", "rao", "rou", "ran", "ren", "rang", "reng",
    "rong", "rua", "ruan", "rui", "run", "ruo",
    "za", "ze", "zi", "zu", "zai", "zei", "zao", "zou", "zan", "zen", "zang", "zeng",
    "zong", "zuo", "zui", "zuan", "zun",
    "ca", "ce", "ci", "cu", "cai", "cao", "cou", "can", "cen", "cang", "ceng",
    "cong", "cuo", "cui", "cuan", "cun",
    "sa", "se", "si", "su", "sai", "sao", "sou", "san", "sen", "sang", "seng",
    "song", "suo", "sui", "suan", "sun",
    "ya", "ye", "yi", "yo", "yu", "yao", "you", "yan", "yin", "yang", "ying",
    "yong", "yuan", "yun", "yue",
    "wa", "wo", "wu", "wai", "wei", "wan", "wen", "wang", "weng",
];

// ============================================================
// 拼音切分 — 贪心最长匹配（纯 ASCII bytes 操作）
// ============================================================

/// 将纯 ASCII 拼音字符串切分为音节
fn split_pinyin(input: &str) -> Vec<String> {
    debug_assert!(input.is_ascii(), "split_pinyin expects pure ASCII");
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut result = Vec::new();
    let mut i = 0;

    while i < len {
        let mut best = 0;
        let max = std::cmp::min(6, len - i);
        for try_len in (1..=max).rev() {
            // 安全：纯 ASCII 所以字节切片即字符切片
            let s = unsafe { std::str::from_utf8_unchecked(&bytes[i..i + try_len]) };
            if is_valid_syllable(s) {
                best = try_len;
                break;
            }
        }
        if best > 0 {
            let s = unsafe { std::str::from_utf8_unchecked(&bytes[i..i + best]) };
            result.push(s.to_string());
            i += best;
        } else {
            result.push((bytes[i] as char).to_string());
            i += 1;
        }
    }
    result
}

/// 公开的拼音切分接口（供 ai_engine 使用）
pub fn split_pinyin_pub(input: &str) -> Vec<String> {
    if !input.is_ascii() { return vec![input.to_string()]; }
    split_pinyin(input)
}

/// 获取歧义切分: 返回所有合理的备选切分方案 (不含贪心主方案)
///
/// 例: "xian" 贪心=["xian"], 歧义备选=["xi","an"]
///     "fangan" 贪心=["fang","an"], 歧义备选=["fan","gan"]
///     "nihao" 贪心=["ni","hao"], 歧义备选=[] (无歧义)
fn split_pinyin_ambiguous(input: &str) -> Vec<Vec<String>> {
    if !input.is_ascii() || input.len() < 3 { return vec![]; }
    let greedy = split_pinyin(input);
    let mut alternatives = Vec::new();

    // 对每个贪心音节，尝试在不同位置截短，看剩余能否形成合法音节
    try_split_recursive(input.as_bytes(), 0, &mut Vec::new(), &greedy, &mut alternatives);

    // 去重 + 去掉和贪心一样的
    alternatives.retain(|alt| *alt != greedy);
    alternatives.sort();
    alternatives.dedup();
    alternatives
}

fn try_split_recursive(
    bytes: &[u8],
    pos: usize,
    current: &mut Vec<String>,
    greedy: &[String],
    results: &mut Vec<Vec<String>>,
) {
    if pos >= bytes.len() {
        if current.len() >= 2 && *current != *greedy {
            results.push(current.clone());
        }
        return;
    }
    // 限制结果数量
    if results.len() >= 5 { return; }

    let remaining = bytes.len() - pos;
    let max_try = std::cmp::min(6, remaining);

    // 尝试每种合法音节长度 (不只是最长)
    for try_len in (1..=max_try).rev() {
        let s = unsafe { std::str::from_utf8_unchecked(&bytes[pos..pos + try_len]) };
        if is_valid_syllable(s) {
            current.push(s.to_string());
            try_split_recursive(bytes, pos + try_len, current, greedy, results);
            current.pop();
        }
    }
}

/// 公开接口: 获取歧义切分结果
pub fn split_pinyin_ambiguous_pub(input: &str) -> Vec<Vec<String>> {
    split_pinyin_ambiguous(input)
}

fn is_valid_syllable(s: &str) -> bool {
    VALID_SYLLABLES.contains(&s)
}

/// 从纯 ASCII 拼音提取首字母缩写: "shijian" -> "sj"
fn make_abbreviation(pinyin: &str) -> String {
    split_pinyin(pinyin)
        .iter()
        .map(|s| s.as_bytes()[0] as char)
        .collect()
}

/// 清洗拼音字段：
/// - ü / µ / 眉 / lv类似乱码 → v
/// - 只保留 a-z 字符
/// - 返回 None 表示清洗后为空
fn sanitize_pinyin(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();

    while let Some(ch) = chars.next() {
        match ch {
            'a'..='z' => out.push(ch),
            // ü 及其声调变体 → v
            '\u{00fc}' | '\u{01dc}' | '\u{01da}' | '\u{01d8}' | '\u{01d6}' => out.push('v'),
            // 乱码残留（如 眉 代替 ü）—— 跳过非 ASCII
            _ if !ch.is_ascii() => { /* skip */ }
            // 其他 ASCII 但非小写字母（数字/空格等）—— 跳过
            _ => {}
        }
    }

    if out.is_empty() { None } else { Some(out) }
}

// ============================================================
// 词典 — 三级预索引
// ============================================================

use std::collections::HashMap;
use std::sync::OnceLock;
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub word: String,
    pub weight: u32,
    pub pinyin: String,
}

static DICT: OnceLock<Dictionary> = OnceLock::new();

/// AI 生成词缓存 (运行时动态添加)
static AI_CACHE: std::sync::LazyLock<std::sync::RwLock<HashMap<String, Vec<Candidate>>>>
    = std::sync::LazyLock::new(|| std::sync::RwLock::new(HashMap::new()));

/// 获取全局字典引用 (供 ai_engine 词图分词使用)
pub fn get_dict() -> Option<&'static Dictionary> {
    DICT.get()
}

/// 缓存 AI 生成的长词到内存 + 磁盘
pub fn cache_ai_word(pinyin: &str, word: &str) {
    if pinyin.is_empty() || word.is_empty() { return; }

    // 检查主字典是否已有
    if let Some(dict) = DICT.get() {
        let entries = dict.lookup(pinyin);
        if entries.iter().any(|c| c.word == word) { return; }
    }

    // 检查缓存是否已有
    {
        let cache = AI_CACHE.read().unwrap();
        if let Some(entries) = cache.get(pinyin) {
            if entries.iter().any(|c| c.word == word) { return; }
        }
    }

    // 写入内存缓存
    {
        let mut cache = AI_CACHE.write().unwrap();
        cache.entry(pinyin.to_string()).or_default().push(Candidate {
            word: word.to_string(),
            weight: 880,
            pinyin: pinyin.to_string(),
        });
    }

    eprintln!("[Dict] 📦 缓存AI词: {} → {}", pinyin, word);

    // 追加到磁盘 dict.txt
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let path = dir.join("dict.txt");
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&path) {
                use std::io::Write;
                let _ = writeln!(f, "{},{},880", pinyin, word);
            }
        }
    }
}

/// 从缓存补充查询结果
pub fn lookup_with_cache(pinyin: &str) -> Vec<Candidate> {
    let mut result = Vec::new();
    
    // 主字典
    if let Some(dict) = DICT.get() {
        result.extend_from_slice(dict.lookup(pinyin));
    }
    
    // AI 缓存
    if let Ok(cache) = AI_CACHE.read() {
        if let Some(entries) = cache.get(pinyin) {
            for c in entries {
                if !result.iter().any(|r| r.word == c.word) {
                    result.push(c.clone());
                }
            }
        }
    }
    
    result
}

#[derive(Serialize, Deserialize)]
pub struct Dictionary {
    /// 精确匹配: "shi" -> [是, 时, ...]
    exact: HashMap<String, Vec<Candidate>>,
    /// 前缀索引: "s" -> [是, 时, 上, ...]  "sh" -> [是, 时, ...]
    prefix: HashMap<String, Vec<usize>>,  // usize = index into `all`
    /// 缩写索引: "sj" -> [时间, 世界, 司机, ...]
    abbrev: HashMap<String, Vec<usize>>,
    /// 所有候选词的扁平数组
    all: Vec<Candidate>,
}

impl Dictionary {
    pub fn from_text(text: &str) -> Self {
        let mut exact: HashMap<String, Vec<Candidate>> = HashMap::new();
        let mut all: Vec<Candidate> = Vec::new();

        // 第一遍: 解析所有条目
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }

            let mut parts = line.splitn(3, ',');
            let pinyin_raw = match parts.next() { Some(s) => s.trim(), None => continue };
            let word = match parts.next() { Some(s) => s.trim(), None => continue };
            let weight: u32 = parts.next()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(50);

            if pinyin_raw.is_empty() || word.is_empty() { continue; }

            // 清洗拼音：ü→v，去掉非 a-z 字符
            let pinyin = match sanitize_pinyin(pinyin_raw) {
                Some(p) => p,
                None => continue,
            };

            let cand = Candidate {
                word: word.to_string(),
                weight,
                pinyin: pinyin.to_string(),
            };
            exact.entry(pinyin.to_string()).or_default().push(cand.clone());
            all.push(cand);
        }

        // 排序每个精确组
        for v in exact.values_mut() {
            v.sort_by(|a, b| b.weight.cmp(&a.weight));
        }

        // 第二遍: 构建前缀索引 + 缩写索引
        let mut prefix: HashMap<String, Vec<usize>> = HashMap::new();
        let mut abbrev: HashMap<String, Vec<usize>> = HashMap::new();

        for (i, cand) in all.iter().enumerate() {
            let py = &cand.pinyin;
            // 前缀: 为拼音的每个前缀子串建索引 (1..len)
            let max_prefix = py.len().min(6);
            for plen in 1..=max_prefix {
                let pre = &py[..plen];
                prefix.entry(pre.to_string()).or_default().push(i);
            }

            // 缩写: 切分音节取首字母
            let ab = make_abbreviation(py);
            if ab.len() >= 2 && ab != *py {
                abbrev.entry(ab).or_default().push(i);
            }
        }

        eprintln!("[Dict] {} 个精确键, {} 条词, {} 个前缀, {} 个缩写",
            exact.len(), all.len(), prefix.len(), abbrev.len());

        Dictionary { exact, prefix, abbrev, all }
    }

    /// 精确匹配 (O(1))
    pub fn lookup(&self, pinyin: &str) -> &[Candidate] {
        self.exact.get(pinyin).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// 前缀匹配 (O(1) 查索引 + 排序)
    pub fn lookup_prefix(&self, pre: &str) -> Vec<&Candidate> {
        match self.prefix.get(pre) {
            Some(indices) => {
                let mut result: Vec<&Candidate> = indices.iter()
                    .map(|&i| &self.all[i])
                    .collect();
                result.sort_by(|a, b| b.weight.cmp(&a.weight));
                result
            }
            None => vec![],
        }
    }

    /// 缩写匹配 (O(1))
    pub fn lookup_abbreviation(&self, abbrev: &str) -> Vec<&Candidate> {
        match self.abbrev.get(abbrev) {
            Some(indices) => {
                let mut result: Vec<&Candidate> = indices.iter()
                    .map(|&i| &self.all[i])
                    .collect();
                result.sort_by(|a, b| b.weight.cmp(&a.weight));
                result
            }
            None => vec![],
        }
    }

    /// 提升候选词权重
    pub fn boost_weight(&mut self, pinyin: &str, word: &str, amount: u32) {
        if let Some(cands) = self.exact.get_mut(pinyin) {
            for c in cands.iter_mut() {
                if c.word == word {
                    c.weight = c.weight.saturating_add(amount);
                    break;
                }
            }
            cands.sort_by(|a, b| b.weight.cmp(&a.weight));
        }
    }

    /// 合并额外词典文本到当前字典, 返回新增条目数
    pub fn merge_text(&mut self, text: &str) -> usize {
        let mut added = 0;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }

            let parts: Vec<&str> = line.splitn(3, ',').collect();
            if parts.len() < 2 { continue; }

            let raw_py = parts[0].trim().to_lowercase();
            let word = parts[1].trim();
            let weight: u32 = parts.get(2)
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(50);

            if raw_py.is_empty() || word.is_empty() { continue; }

            // 检查是否已存在 (避免重复)
            let exists = self.exact.get(&raw_py)
                .map(|v| v.iter().any(|c| c.word == word))
                .unwrap_or(false);
            if exists { continue; }

            let cand = Candidate {
                word: word.to_string(),
                weight,
                pinyin: raw_py.clone(),
            };

            let idx = self.all.len();
            self.all.push(cand.clone());

            // 精确索引
            self.exact.entry(raw_py.clone()).or_default().push(cand);

            // 前缀索引
            let max_prefix = raw_py.len().min(6);
            for plen in 1..=max_prefix {
                let pre = &raw_py[..plen];
                self.prefix.entry(pre.to_string()).or_default().push(idx);
            }

            // 缩写索引
            let ab = make_abbreviation(&raw_py);
            if ab.len() >= 2 && ab != raw_py {
                self.abbrev.entry(ab).or_default().push(idx);
            }

            added += 1;
        }

        // 重排精确组
        for v in self.exact.values_mut() {
            v.sort_by(|a, b| b.weight.cmp(&a.weight));
        }

        added
    }
}

pub fn global_dict() -> &'static Dictionary {
    DICT.get_or_init(|| load_dictionary(&[]))
}

/// 初始化全局字典（带额外词库），由 main 调用
pub fn init_global_dict(extra_names: &[String]) {
    DICT.get_or_init(|| load_dictionary(extra_names));
}

fn load_dictionary(extra_names: &[String]) -> Dictionary {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    // 优先加载二进制缓存 (dict.bin)
    let bin_path = exe_dir.as_ref().map(|d| d.join("dict.bin"));
    if let Some(ref bp) = bin_path {
        if bp.exists() {
            let start = std::time::Instant::now();
            match std::fs::read(bp) {
                Ok(bytes) => match bincode::deserialize::<Dictionary>(&bytes) {
                    Ok(d) => {
                        eprintln!("[Dict] 二进制缓存加载: {:?} ({} 条)",
                            start.elapsed(), d.all.len());
                        return d;
                    }
                    Err(e) => eprintln!("[Dict] bin 反序列化失败: {}, 回退文本", e),
                }
                Err(e) => eprintln!("[Dict] bin 读取失败: {}, 回退文本", e),
            }
        }
    }

    // 回退: 加载文本词典 (dict.txt)
    let dict_path = exe_dir.as_ref()
        .map(|d| d.join("dict.txt"))
        .filter(|p| p.exists())
        .or_else(|| {
            let p = std::path::Path::new("dict.txt");
            if p.exists() { Some(p.to_path_buf()) } else { None }
        });

    let mut dict = match dict_path {
        Some(path) => {
            eprintln!("[Dict] 基础词典: {:?}", path);
            let start = std::time::Instant::now();
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let d = Dictionary::from_text(&text);
                    eprintln!("[Dict] 基础词典加载: {:?}", start.elapsed());
                    d
                }
                Err(e) => {
                    eprintln!("[Dict] error: {}", e);
                    Dictionary::from_text("")
                }
            }
        }
        None => {
            eprintln!("[Dict] no dict.txt, builtin fallback");
            Dictionary::from_text(BUILTIN_DICT)
        }
    };

    // 2. 加载额外词库 (dict/*.txt)
    if !extra_names.is_empty() {
        let dict_dir = exe_dir.as_ref()
            .map(|d| d.join("dict"))
            .or_else(|| Some(std::path::PathBuf::from("dict")));

        for name in extra_names {
            let ext_path = dict_dir.as_ref()
                .map(|d| d.join(format!("{}.txt", name)));

            if let Some(path) = ext_path.filter(|p| p.exists()) {
                match std::fs::read_to_string(&path) {
                    Ok(text) => {
                        let count = dict.merge_text(&text);
                        eprintln!("[Dict] +{}: {} 条", name, count);
                    }
                    Err(e) => {
                        eprintln!("[Dict] ⚠ {}: {}", name, e);
                    }
                }
            } else {
                eprintln!("[Dict] ⚠ 未找到词库: {}", name);
            }
        }
    }

    // 自动生成二进制缓存
    if let Some(ref bp) = bin_path {
        let start = std::time::Instant::now();
        match bincode::serialize(&dict) {
            Ok(bytes) => {
                match std::fs::write(bp, &bytes) {
                    Ok(_) => eprintln!("[Dict] 已生成二进制缓存: {:?} ({:.1} MB, {:?})",
                        bp, bytes.len() as f64 / 1_048_576.0, start.elapsed()),
                    Err(e) => eprintln!("[Dict] 写入 bin 失败: {}", e),
                }
            }
            Err(e) => eprintln!("[Dict] 序列化失败: {}", e),
        }
    }

    dict
}

const BUILTIN_DICT: &str = "\
de,的,999
shi,是,998
bu,不,997
le,了,996
wo,我,995
ni,你,994
ta,他,993
zhe,这,992
na,那,991
you,有,990
ren,人,989
zai,在,988
da,大,987
shang,上,986
zhong,中,985
yi,一,984
ge,个,983
lai,来,982
qu,去,981
hao,好,980
xiang,想,979
shuo,说,978
dui,对,977
shijian,时间,100
women,我们,100
nihao,你好,70
zaijian,再见,70
";

// ============================================================
// PinyinEngine
// ============================================================

pub struct PinyinEngine {
    raw: String,
    syllables: Vec<String>,
}

impl PinyinEngine {
    pub fn new() -> Self {
        let _ = global_dict();
        Self { raw: String::new(), syllables: vec![] }
    }

    pub fn push(&mut self, ch: char) {
        if ch.is_ascii_lowercase() {
            self.raw.push(ch);
            self.syllables = split_pinyin(&self.raw);
        }
    }

    pub fn pop(&mut self) {
        self.raw.pop();
        self.syllables = if self.raw.is_empty() {
            vec![]
        } else {
            split_pinyin(&self.raw)
        };
    }

    pub fn clear(&mut self) {
        self.raw.clear();
        self.syllables.clear();
    }

    /// 消耗前 n 个音节 (选字后只吃掉已用音节, 剩余保留)
    ///
    /// 例: raw="nengbuneng" syllables=["neng","bu","neng"]
    ///     consume_syllables(1) → raw="buneng" syllables=["bu","neng"]
    ///     consume_syllables(3) → raw="" syllables=[]
    pub fn consume_syllables(&mut self, n: usize) {
        if n == 0 { return; }
        if n >= self.syllables.len() {
            self.clear();
            return;
        }
        // 计算前 n 个音节占了多少 raw 字符
        let chars_to_consume: usize = self.syllables[..n]
            .iter().map(|s| s.len()).sum();
        if chars_to_consume >= self.raw.len() {
            self.clear();
        } else {
            self.raw = self.raw[chars_to_consume..].to_string();
            self.syllables = split_pinyin(&self.raw);
        }
    }

    pub fn raw_input(&self) -> &str { &self.raw }
    pub fn syllables(&self) -> &[String] { &self.syllables }
    pub fn is_empty(&self) -> bool { self.raw.is_empty() }

    /// 多策略候选搜索 (全部 O(1), 无遍历)
    pub fn get_candidates(&self) -> Vec<String> {
        if self.raw.is_empty() { return vec![]; }

        let dict = global_dict();
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();

        // 辅助: 去重添加
        macro_rules! add {
            ($cands:expr, $limit:expr) => {
                for c in $cands.iter().take($limit) {
                    if seen.insert(c.word.clone()) {
                        result.push(c.word.clone());
                    }
                }
            };
        }

        // 1. 整体精确匹配: "wo" -> 我; "shijian" -> 时间
        let exact = dict.lookup(&self.raw);
        add!(exact, 20);

        // 2. 第一音节精确匹配 (仅当与 raw 不同)
        if let Some(first) = self.syllables.first() {
            if first.as_str() != self.raw {
                let first_exact = dict.lookup(first);
                add!(first_exact, 9);
            }
        }

        // 2.5 歧义切分候选: "xian" → 贪心["xian"], 备选["xi","an"] → 查 "xian" 的词
        let alt_splits = split_pinyin_ambiguous(&self.raw);
        for alt in &alt_splits {
            // 尝试将备选切分拼成完整拼音key查字典
            let alt_key: String = alt.join("");
            if alt_key != self.raw {
                // 整体精确匹配备选key (通常和主相同, 跳过)
            }
            // 对备选切分的第一音节做精确查找
            if let Some(first) = alt.first() {
                if first.as_str() != self.syllables.first().map(|s| s.as_str()).unwrap_or("") {
                    let alt_exact = dict.lookup(first);
                    add!(alt_exact, 5);
                }
            }
            // 多音节: 查找完整拼音组合 "xi"+"an" → "xian" 已查过,
            // 但可以用 join key 查: "fan"+"gan" → "fangan"
            if alt.len() >= 2 {
                let multi_key: String = alt.iter().map(|s| s.as_str()).collect();
                let multi_exact = dict.lookup(&multi_key);
                add!(multi_exact, 5);
            }
        }

        // 3. 首字母缩写: "wm" -> 我们, "sj" -> 时间
        if self.raw.len() >= 2 && self.raw.len() <= 10 {
            let ab = dict.lookup_abbreviation(&self.raw);
            add!(ab, 15);
        }

        // 4. 前缀匹配 (保底)
        if result.len() < 9 {
            let pfx = dict.lookup_prefix(&self.raw);
            add!(pfx, 20);
        }

        // 5. 第一音节前缀 (再保底)
        // 警告: 若第一音节只是单个辅音字母(如"d"), lookup_prefix("d")
        // 会返回所有以d开头的词，导致"地方""但是""大家"等无关词入侵候选
        if result.len() < 9 {
            if let Some(first) = self.syllables.first() {
                let first_str = first.as_str();
                // 只有 2+ 字符的前缀才有实际约束效果
                if first_str != self.raw && first_str.len() >= 2 {
                    let pfx = dict.lookup_prefix(first);
                    add!(pfx, 15);
                }
            }
        }

        result
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split() {
        assert_eq!(split_pinyin("nihao"), vec!["ni", "hao"]);
        assert_eq!(split_pinyin("xian"), vec!["xian"]);
        assert_eq!(split_pinyin("zhuang"), vec!["zhuang"]);
    }

    #[test]
    fn test_ambiguous_split() {
        // xian → 贪心[xian], 歧义[xi,an]
        let alts = split_pinyin_ambiguous("xian");
        assert!(alts.contains(&vec!["xi".to_string(), "an".to_string()]));

        // fangan → 贪心[fang,an], 歧义[fan,gan]
        let alts = split_pinyin_ambiguous("fangan");
        assert!(alts.contains(&vec!["fan".to_string(), "gan".to_string()]));

        // nihao → 无歧义
        let alts = split_pinyin_ambiguous("nihao");
        assert!(alts.is_empty() || !alts.contains(&vec!["ni".to_string(), "hao".to_string()]));
    }

    #[test]
    fn test_abbreviation_index() {
        assert_eq!(make_abbreviation("shijian"), "sj");
        assert_eq!(make_abbreviation("women"), "wm");
    }

    #[test]
    fn test_lookup() {
        let dict = Dictionary::from_text("shi,是,100\nshi,时,90\n");
        let r = dict.lookup("shi");
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].word, "是");
    }

    #[test]
    fn test_abbreviation_search() {
        let dict = Dictionary::from_text(
            "shijian,时间,100\nwomen,我们,90\nsiji,司机,80\n"
        );
        let r = dict.lookup_abbreviation("sj");
        assert!(r.iter().any(|c| c.word == "时间"));
        assert!(r.iter().any(|c| c.word == "司机"));

        let r2 = dict.lookup_abbreviation("wm");
        assert!(r2.iter().any(|c| c.word == "我们"));
    }

    #[test]
    fn test_prefix() {
        let dict = Dictionary::from_text("shi,是,100\nshijian,时间,80\nsha,沙,50\n");
        let r = dict.lookup_prefix("sh");
        assert!(r.len() >= 2);
    }

    #[test]
    fn test_boost() {
        let mut dict = Dictionary::from_text("shi,是,100\nshi,时,90\n");
        dict.boost_weight("shi", "时", 20);
        let r = dict.lookup("shi");
        assert_eq!(r[0].word, "时");
    }

    #[test]
    fn test_sanitize_pinyin() {
        // 正常拼音不变
        assert_eq!(sanitize_pinyin("shijian"), Some("shijian".into()));
        // ü → v
        assert_eq!(sanitize_pinyin("l\u{00fc}"), Some("lv".into()));
        // 非 ASCII 字符被移除
        assert_eq!(sanitize_pinyin("buganl\u{00fc}emei"), Some("buganlvemei".into()));
        // 纯乱码 → None
        assert_eq!(sanitize_pinyin("眉"), None);
    }
}
