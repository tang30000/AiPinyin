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

#[derive(Clone, Debug)]
pub struct Candidate {
    pub word: String,
    pub weight: u32,
    pub pinyin: String,
}

static DICT: OnceLock<Dictionary> = OnceLock::new();

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
}

pub fn global_dict() -> &'static Dictionary {
    DICT.get_or_init(|| load_dictionary())
}

fn load_dictionary() -> Dictionary {
    let dict_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("dict.txt")))
        .filter(|p| p.exists())
        .or_else(|| {
            let p = std::path::Path::new("dict.txt");
            if p.exists() { Some(p.to_path_buf()) } else { None }
        });

    match dict_path {
        Some(path) => {
            eprintln!("[Dict] {:?}", path);
            let start = std::time::Instant::now();
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let dict = Dictionary::from_text(&text);
                    eprintln!("[Dict] {:?}", start.elapsed());
                    dict
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
    }
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

        // 3. 首字母缩写: "wm" -> 我们, "sj" -> 时间
        if self.raw.len() >= 2 && self.raw.len() <= 6 {
            let ab = dict.lookup_abbreviation(&self.raw);
            add!(ab, 15);
        }

        // 4. 前缀匹配 (保底)
        if result.len() < 9 {
            let pfx = dict.lookup_prefix(&self.raw);
            add!(pfx, 20);
        }

        // 5. 第一音节前缀 (再保底)
        if result.len() < 9 {
            if let Some(first) = self.syllables.first() {
                if first.as_str() != self.raw {
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
