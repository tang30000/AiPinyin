//! # 用户自学习词典
//!
//! 记录用户的选词行为，自动调整候选排序。
//!
//! ## 机制
//! - 每次用户选词上屏时记录 (拼音, 汉字, 次数)
//! - 数据持久化到 `user_dict.txt`（exe 同目录）
//! - 启动时加载，选词时增量写入
//! - 权重会叠加到主词典的查询结果中

use std::collections::HashMap;
use std::path::PathBuf;
use std::io::Write;

/// 用户自学习词典
pub struct UserDict {
    /// (拼音, 汉字) -> 使用次数
    entries: HashMap<(String, String), u32>,
    /// 文件路径
    path: PathBuf,
    /// 脏标记：是否有未保存的修改
    dirty: bool,
}

impl UserDict {
    /// 加载或创建用户词典
    pub fn load() -> Self {
        let path = Self::dict_path();
        let mut entries = HashMap::new();

        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    for line in text.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') { continue; }
                        // 格式: 拼音\t汉字\t次数
                        let parts: Vec<&str> = line.split('\t').collect();
                        if parts.len() >= 3 {
                            let pinyin = parts[0].to_string();
                            let word = parts[1].to_string();
                            let count: u32 = parts[2].parse().unwrap_or(1);
                            entries.insert((pinyin, word), count);
                        }
                    }
                    eprintln!("[UserDict] ✅ 已加载 {} 条用户词 {:?}", entries.len(), path);
                }
                Err(e) => {
                    eprintln!("[UserDict] ⚠ 读取失败: {}", e);
                }
            }
        } else {
            eprintln!("[UserDict] ℹ user_dict.txt 不存在, 将在学习时创建");
        }

        Self { entries, path, dirty: false }
    }

    /// 学习一次选词：增加计数，如果是新词则添加
    pub fn learn(&mut self, pinyin: &str, word: &str) {
        if pinyin.is_empty() || word.is_empty() { return; }

        let key = (pinyin.to_string(), word.to_string());
        let count = self.entries.entry(key).or_insert(0);
        *count += 1;
        self.dirty = true;

        eprintln!("[UserDict] 📝 学习 {} → {} (count={})", pinyin, word, count);

        // 每次学习都增量保存（简单可靠）
        self.save();
    }

    /// 获取某个词的用户权重（0 = 未学习过）
    pub fn get_weight(&self, pinyin: &str, word: &str) -> u32 {
        let key = (pinyin.to_string(), word.to_string());
        self.entries.get(&key).copied().unwrap_or(0)
    }

    /// 获取某个拼音下所有用户学过的词（用于补充候选）
    pub fn get_learned_words(&self, pinyin: &str) -> Vec<(String, u32)> {
        let mut result: Vec<(String, u32)> = self.entries.iter()
            .filter(|((py, _), _)| py == pinyin)
            .map(|((_, word), &count)| (word.clone(), count))
            .collect();
        result.sort_by(|a, b| b.1.cmp(&a.1));
        result
    }

    /// 保存到文件
    fn save(&mut self) {
        if !self.dirty { return; }

        match std::fs::File::create(&self.path) {
            Ok(mut f) => {
                let _ = writeln!(f, "# AiPinyin 用户词典 — 自动生成，请勿手动编辑");
                let _ = writeln!(f, "# 格式: 拼音\\t汉字\\t次数");

                // 按次数降序排列
                let mut sorted: Vec<_> = self.entries.iter().collect();
                sorted.sort_by(|a, b| b.1.cmp(a.1));

                for ((pinyin, word), count) in &sorted {
                    let _ = writeln!(f, "{}\t{}\t{}", pinyin, word, count);
                }

                self.dirty = false;
            }
            Err(e) => {
                eprintln!("[UserDict] ⚠ 保存失败: {}", e);
            }
        }
    }

    /// 用户词典路径（exe 同目录）
    fn dict_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("user_dict.txt")))
            .unwrap_or_else(|| PathBuf::from("user_dict.txt"))
    }
}
