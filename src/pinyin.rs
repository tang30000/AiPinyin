//! # 拼音解析引擎
//!
//! 将连续的英文字母按拼音规则切分为音节，
//! 并通过词典系统提供拼音→汉字候选查找能力。
//!
//! ## 设计
//! - 贪心最长匹配切分
//! - HashMap 词典 + 多策略搜索（精确/前缀/首字母缩写）
//! - 权重排序 + 动态词频提升


// ============================================================
// 拼音合法音节表（完整声母+韵母组合）
// ============================================================

/// 所有合法拼音音节（不含声调）
const VALID_SYLLABLES: &[&str] = &[
    // 单韵母
    "a", "o", "e", "ai", "ei", "ao", "ou", "an", "en", "ang", "eng", "er",
    // b
    "ba", "bo", "bi", "bu", "bai", "bei", "bao", "ban", "ben", "bang", "beng",
    "bie", "biao", "bian", "bin", "bing",
    // p
    "pa", "po", "pi", "pu", "pai", "pei", "pao", "pou", "pan", "pen", "pang", "peng",
    "pie", "piao", "pian", "pin", "ping",
    // m
    "ma", "mo", "me", "mi", "mu", "mai", "mei", "mao", "mou", "man", "men",
    "mang", "meng", "mie", "miao", "miu", "mian", "min", "ming",
    // f
    "fa", "fo", "fu", "fei", "fou", "fan", "fen", "fang", "feng",
    // d
    "da", "de", "di", "du", "dai", "dei", "dao", "dou", "dan", "den", "dang", "deng",
    "dong", "die", "diao", "diu", "dian", "ding", "duo", "dui", "duan", "dun",
    // t
    "ta", "te", "ti", "tu", "tai", "tao", "tou", "tan", "tang", "teng",
    "tong", "tie", "tiao", "tian", "ting", "tuo", "tui", "tuan", "tun",
    // n
    "na", "ne", "ni", "nu", "nv", "nai", "nei", "nao", "nou", "nan", "nen",
    "nang", "neng", "nong", "nie", "niao", "niu", "nian", "nin", "ning",
    "nuo", "nuan", "nve",
    // l
    "la", "le", "li", "lu", "lv", "lai", "lei", "lao", "lou", "lan", "lang", "leng",
    "long", "lie", "liao", "liu", "lian", "lin", "ling", "luo", "luan", "lun", "lve",
    // g
    "ga", "ge", "gu", "gai", "gei", "gao", "gou", "gan", "gen", "gang", "geng",
    "gong", "gua", "guai", "guan", "guang", "gui", "gun", "guo",
    // k
    "ka", "ke", "ku", "kai", "kei", "kao", "kou", "kan", "ken", "kang", "keng",
    "kong", "kua", "kuai", "kuan", "kuang", "kui", "kun", "kuo",
    // h
    "ha", "he", "hu", "hai", "hei", "hao", "hou", "han", "hen", "hang", "heng",
    "hong", "hua", "huai", "huan", "huang", "hui", "hun", "huo",
    // j
    "ji", "ju", "jia", "jie", "jiao", "jiu", "jian", "jin", "jiang", "jing",
    "jiong", "juan", "jun", "jue",
    // q
    "qi", "qu", "qia", "qie", "qiao", "qiu", "qian", "qin", "qiang", "qing",
    "qiong", "quan", "qun", "que",
    // x
    "xi", "xu", "xia", "xie", "xiao", "xiu", "xian", "xin", "xiang", "xing",
    "xiong", "xuan", "xun", "xue",
    // zh
    "zha", "zhe", "zhi", "zhu", "zhai", "zhei", "zhao", "zhou", "zhan", "zhen",
    "zhang", "zheng", "zhong", "zhua", "zhuai", "zhuan", "zhuang", "zhui", "zhun", "zhuo",
    // ch
    "cha", "che", "chi", "chu", "chai", "chao", "chou", "chan", "chen",
    "chang", "cheng", "chong", "chua", "chuai", "chuan", "chuang", "chui", "chun", "chuo",
    // sh
    "sha", "she", "shi", "shu", "shai", "shei", "shao", "shou", "shan", "shen",
    "shang", "sheng", "shua", "shuai", "shuan", "shuang", "shui", "shun", "shuo",
    // r
    "re", "ri", "ru", "rao", "rou", "ran", "ren", "rang", "reng",
    "rong", "rua", "ruan", "rui", "run", "ruo",
    // z
    "za", "ze", "zi", "zu", "zai", "zei", "zao", "zou", "zan", "zen", "zang", "zeng",
    "zong", "zuo", "zui", "zuan", "zun",
    // c
    "ca", "ce", "ci", "cu", "cai", "cao", "cou", "can", "cen", "cang", "ceng",
    "cong", "cuo", "cui", "cuan", "cun",
    // s
    "sa", "se", "si", "su", "sai", "sao", "sou", "san", "sen", "sang", "seng",
    "song", "suo", "sui", "suan", "sun",
    // y
    "ya", "ye", "yi", "yo", "yu", "yao", "you", "yan", "yin", "yang", "ying",
    "yong", "yuan", "yun", "yue",
    // w
    "wa", "wo", "wu", "wai", "wei", "wan", "wen", "wang", "weng",
];

// ============================================================
// 拼音切分算法 — 贪心最长匹配
// ============================================================

/// 将连续字母序列切分为拼音音节（贪心最长匹配）
///
/// 例如: "nihao" -> ["ni", "hao"]
///       "xian"  -> ["xian"]（不是 "xi" + "an"）
fn split_pinyin(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let mut best_len = 0;
        let max_try = std::cmp::min(6, len - i);
        for try_len in (1..=max_try).rev() {
            let candidate: String = chars[i..i + try_len].iter().collect();
            if is_valid_syllable(&candidate) {
                best_len = try_len;
                break;
            }
        }

        if best_len > 0 {
            let syllable: String = chars[i..i + best_len].iter().collect();
            result.push(syllable);
            i += best_len;
        } else {
            result.push(chars[i].to_string());
            i += 1;
        }
    }

    result
}

/// 检查是否为合法拼音音节
fn is_valid_syllable(s: &str) -> bool {
    VALID_SYLLABLES.binary_search(&s).is_ok()
        || VALID_SYLLABLES.contains(&s)
}

// ============================================================
// 词典系统 — HashMap<拼音, Vec<Candidate>>
// ============================================================

use std::collections::HashMap;
use std::sync::OnceLock;

/// 单条候选词
#[derive(Clone, Debug)]
pub struct Candidate {
    pub word: String,
    pub weight: u32,
    pub pinyin: String,
}

/// 全局词典（线程安全，只初始化一次）
static DICT: OnceLock<Dictionary> = OnceLock::new();

pub struct Dictionary {
    map: HashMap<String, Vec<Candidate>>,
}

impl Dictionary {
    /// 从 dict.txt 格式加载: 拼音,汉字,权重
    pub fn from_text(text: &str) -> Self {
        let mut map: HashMap<String, Vec<Candidate>> = HashMap::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }

            let parts: Vec<&str> = line.splitn(3, ',').collect();
            if parts.len() < 3 { continue; }

            let pinyin = parts[0].trim().to_string();
            let word = parts[1].trim().to_string();
            let weight: u32 = parts[2].trim().parse().unwrap_or(50);

            if pinyin.is_empty() || word.is_empty() { continue; }

            map.entry(pinyin.clone())
                .or_default()
                .push(Candidate { word, weight, pinyin });
        }

        // 按权重排序
        for cands in map.values_mut() {
            cands.sort_by(|a, b| b.weight.cmp(&a.weight));
        }

        eprintln!("[Dict] {} 个拼音键, {} 条候选词",
            map.len(),
            map.values().map(|v| v.len()).sum::<usize>());

        Dictionary { map }
    }

    /// 精确匹配
    pub fn lookup(&self, pinyin: &str) -> Vec<&Candidate> {
        self.map.get(pinyin)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// 前缀匹配
    pub fn lookup_prefix(&self, prefix: &str) -> Vec<&Candidate> {
        let mut result = Vec::new();
        for (key, cands) in &self.map {
            if key.starts_with(prefix) {
                result.extend(cands.iter());
            }
        }
        result.sort_by(|a, b| b.weight.cmp(&a.weight));
        result
    }

    /// 首字母缩写匹配: "sj" -> "shijian"
    pub fn lookup_abbreviation(&self, abbrev: &str) -> Vec<&Candidate> {
        let abbrev_chars: Vec<char> = abbrev.chars().collect();
        let mut result = Vec::new();

        for (key, cands) in &self.map {
            let syllables = split_pinyin(key);
            if syllables.len() != abbrev_chars.len() { continue; }

            let matches = syllables.iter().zip(abbrev_chars.iter())
                .all(|(syl, &ch)| syl.starts_with(ch));

            if matches {
                result.extend(cands.iter());
            }
        }

        result.sort_by(|a, b| b.weight.cmp(&a.weight));
        result
    }

    /// 提升候选词权重（动态词频）
    pub fn boost_weight(&mut self, pinyin: &str, word: &str, amount: u32) {
        if let Some(cands) = self.map.get_mut(pinyin) {
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

/// 获取全局词典实例
pub fn global_dict() -> &'static Dictionary {
    DICT.get_or_init(|| load_dictionary())
}

/// 加载词典
fn load_dictionary() -> Dictionary {
    let dict_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("dict.txt")))
        .filter(|p| p.exists())
        .or_else(|| {
            let cwd = std::path::Path::new("dict.txt");
            if cwd.exists() { Some(cwd.to_path_buf()) } else { None }
        });

    match dict_path {
        Some(path) => {
            eprintln!("[Dict] {:?} ...", path);
            let start = std::time::Instant::now();
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let dict = Dictionary::from_text(&text);
                    eprintln!("[Dict] {:?}", start.elapsed());
                    dict
                }
                Err(e) => {
                    eprintln!("[Dict] read error: {}", e);
                    Dictionary::from_text("")
                }
            }
        }
        None => {
            eprintln!("[Dict] no dict.txt, using builtin");
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
nihao,你好,70
zaijian,再见,70
";

// ============================================================
// PinyinEngine — 多策略候选引擎
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

    /// 多策略候选搜索
    pub fn get_candidates(&self) -> Vec<String> {
        if self.raw.is_empty() { return vec![]; }

        let dict = global_dict();
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();

        // 辅助:添加候选并去重
        let mut add_cands = |cands: Vec<&Candidate>| {
            for c in cands {
                if seen.insert(c.word.clone()) {
                    result.push(c.word.clone());
                }
            }
        };

        // 1. 完整拼音精确匹配
        add_cands(dict.lookup(&self.raw));

        // 2. 第一音节精确匹配
        if let Some(first) = self.syllables.first() {
            if first != &self.raw {
                add_cands(dict.lookup(first));
            }
        }

        // 3. 首字母缩写匹配
        if self.raw.len() >= 2 && self.raw.len() <= 5 {
            let abbrev = dict.lookup_abbreviation(&self.raw);
            add_cands(abbrev.into_iter().take(20).collect());
        }

        // 下面的策略不能用闭包了（borrow checker），直接内联
        // 4. 前缀匹配（保底）
        if result.len() < 9 {
            let prefix_cands = dict.lookup_prefix(&self.raw);
            for c in prefix_cands.into_iter().take(20) {
                if seen.insert(c.word.clone()) {
                    result.push(c.word.clone());
                }
            }
        }

        // 5. 第一音节前缀匹配
        if result.len() < 9 {
            if let Some(first) = self.syllables.first() {
                let prefix_cands = dict.lookup_prefix(first);
                for c in prefix_cands.into_iter().take(20) {
                    if seen.insert(c.word.clone()) {
                        result.push(c.word.clone());
                    }
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
    fn test_split_simple() {
        assert_eq!(split_pinyin("nihao"), vec!["ni", "hao"]);
        assert_eq!(split_pinyin("zhongguo"), vec!["zhong", "guo"]);
    }

    #[test]
    fn test_split_greedy() {
        assert_eq!(split_pinyin("xian"), vec!["xian"]);
        assert_eq!(split_pinyin("zhuang"), vec!["zhuang"]);
    }

    #[test]
    fn test_dictionary_lookup() {
        let dict = Dictionary::from_text("shi,是,100\nshi,时,90\n");
        let r = dict.lookup("shi");
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].word, "是");
    }

    #[test]
    fn test_abbreviation() {
        let dict = Dictionary::from_text("shijian,时间,100\nshijie,世界,90\nsiji,司机,80\n");
        let r = dict.lookup_abbreviation("sj");
        assert!(r.iter().any(|c| c.word == "时间"));
        assert!(r.iter().any(|c| c.word == "世界"));
    }

    #[test]
    fn test_boost() {
        let mut dict = Dictionary::from_text("shi,是,100\nshi,时,90\n");
        dict.boost_weight("shi", "时", 20);
        let r = dict.lookup("shi");
        assert_eq!(r[0].word, "时"); // 110 > 100
    }
}
