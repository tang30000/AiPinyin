"""
AiPinyin Tokenizer — 拼音/汉字双向编码器 (v2)

修复: 基于语料频率构建汉字词表，而非 Unicode 顺序取前 N 个。

职责:
  1. 拼音音节 → token ID  (约 415 个)
  2. 汉字字符 → token ID  (基于语料频率)
  3. 提供训练数据的编码/解码接口

特殊 Token:
  0 = PAD, 1 = UNK, 2 = BOS, 3 = EOS
"""

import json
import os
from typing import List, Dict, Optional
from collections import Counter

# ============================================================
# 全拼音节表 (410+)
# ============================================================

PINYIN_SYLLABLES = [
    # 单韵母
    "a", "o", "e", "ai", "ei", "ao", "ou", "an", "en", "ang", "eng", "er",
    # b
    "ba", "bo", "bi", "bu", "bai", "bei", "bao", "ban", "ben", "bang", "beng",
    "bie", "biao", "bian", "bin", "bing",
    # p
    "pa", "po", "pi", "pu", "pai", "pei", "pao", "pou", "pan", "pen", "pang", "peng",
    "pie", "piao", "pian", "pin", "ping",
    # m
    "ma", "mo", "me", "mi", "mu", "mai", "mei", "mao", "mou", "man", "men",
    "mang", "meng", "mie", "miao", "miu", "mian", "min", "ming",
    # f
    "fa", "fo", "fu", "fei", "fou", "fan", "fen", "fang", "feng",
    # d
    "da", "de", "di", "du", "dai", "dei", "dao", "dou", "dan", "den", "dang", "deng",
    "dong", "die", "diao", "diu", "dian", "ding", "duo", "dui", "duan", "dun",
    # t
    "ta", "te", "ti", "tu", "tai", "tao", "tou", "tan", "tang", "teng",
    "tong", "tie", "tiao", "tian", "ting", "tuo", "tui", "tuan", "tun",
    # n
    "na", "ne", "ni", "nu", "nv", "nai", "nei", "nao", "nou", "nan", "nen",
    "nang", "neng", "nong", "nie", "niao", "niu", "nian", "nin", "ning",
    "nuo", "nuan", "nve",
    # l
    "la", "le", "li", "lu", "lv", "lai", "lei", "lao", "lou", "lan", "lang", "leng",
    "long", "lie", "liao", "liu", "lian", "lin", "ling", "luo", "luan", "lun", "lve",
    # g
    "ga", "ge", "gu", "gai", "gei", "gao", "gou", "gan", "gen", "gang", "geng",
    "gong", "gua", "guai", "guan", "guang", "gui", "gun", "guo",
    # k
    "ka", "ke", "ku", "kai", "kei", "kao", "kou", "kan", "ken", "kang", "keng",
    "kong", "kua", "kuai", "kuan", "kuang", "kui", "kun", "kuo",
    # h
    "ha", "he", "hu", "hai", "hei", "hao", "hou", "han", "hen", "hang", "heng",
    "hong", "hua", "huai", "huan", "huang", "hui", "hun", "huo",
    # j
    "ji", "ju", "jia", "jie", "jiao", "jiu", "jian", "jin", "jiang", "jing",
    "jiong", "juan", "jun", "jue",
    # q
    "qi", "qu", "qia", "qie", "qiao", "qiu", "qian", "qin", "qiang", "qing",
    "qiong", "quan", "qun", "que",
    # x
    "xi", "xu", "xia", "xie", "xiao", "xiu", "xian", "xin", "xiang", "xing",
    "xiong", "xuan", "xun", "xue",
    # zh
    "zha", "zhe", "zhi", "zhu", "zhai", "zhei", "zhao", "zhou", "zhan", "zhen",
    "zhang", "zheng", "zhong", "zhua", "zhuai", "zhuan", "zhuang", "zhui", "zhun", "zhuo",
    # ch
    "cha", "che", "chi", "chu", "chai", "chao", "chou", "chan", "chen",
    "chang", "cheng", "chong", "chua", "chuai", "chuan", "chuang", "chui", "chun", "chuo",
    # sh
    "sha", "she", "shi", "shu", "shai", "shei", "shao", "shou", "shan", "shen",
    "shang", "sheng", "shua", "shuai", "shuan", "shuang", "shui", "shun", "shuo",
    # r
    "re", "ri", "ru", "rao", "rou", "ran", "ren", "rang", "reng",
    "rong", "rua", "ruan", "rui", "run", "ruo",
    # z
    "za", "ze", "zi", "zu", "zai", "zei", "zao", "zou", "zan", "zen", "zang", "zeng",
    "zong", "zuo", "zui", "zuan", "zun",
    # c
    "ca", "ce", "ci", "cu", "cai", "cao", "cou", "can", "cen", "cang", "ceng",
    "cong", "cuo", "cui", "cuan", "cun",
    # s
    "sa", "se", "si", "su", "sai", "sao", "sou", "san", "sen", "sang", "seng",
    "song", "suo", "sui", "suan", "sun",
    # y
    "ya", "ye", "yi", "yo", "yu", "yao", "you", "yan", "yin", "yang", "ying",
    "yong", "yuan", "yun", "yue",
    # w
    "wa", "wo", "wu", "wai", "wei", "wan", "wen", "wang", "weng",
]

# 首字母 (支持简拼输入, 如 "n h" → "你好")
PINYIN_INITIALS = list("abcdefghijklmnopqrstuvwxyz")

# 特殊 Token
PAD_TOKEN = "<PAD>"
UNK_TOKEN = "<UNK>"
BOS_TOKEN = "<BOS>"
EOS_TOKEN = "<EOS>"
SPECIAL_TOKENS = [PAD_TOKEN, UNK_TOKEN, BOS_TOKEN, EOS_TOKEN]

PAD_ID = 0
UNK_ID = 1
BOS_ID = 2
EOS_ID = 3


class PinyinTokenizer:
    """拼音音节编码器"""

    def __init__(self):
        self.syl2id: Dict[str, int] = {}
        self.id2syl: Dict[int, str] = {}
        self._build()

    def _build(self):
        for i, tok in enumerate(SPECIAL_TOKENS):
            self.syl2id[tok] = i
            self.id2syl[i] = tok
        offset = len(SPECIAL_TOKENS)
        idx = 0
        # 先加单字母 (首字母简拼)
        for letter in PINYIN_INITIALS:
            if letter not in self.syl2id:
                self.syl2id[letter] = offset + idx
                self.id2syl[offset + idx] = letter
                idx += 1
        # 再加完整音节
        for syl in PINYIN_SYLLABLES:
            if syl not in self.syl2id:
                self.syl2id[syl] = offset + idx
                self.id2syl[offset + idx] = syl
                idx += 1

    @property
    def vocab_size(self) -> int:
        return len(self.syl2id)

    def encode(self, syllables: List[str]) -> List[int]:
        """音节列表 → token ID 列表"""
        return [self.syl2id.get(s, UNK_ID) for s in syllables]

    def decode(self, ids: List[int]) -> List[str]:
        return [self.id2syl.get(i, UNK_TOKEN) for i in ids]

    def save(self, path: str):
        with open(path, "w", encoding="utf-8") as f:
            json.dump(self.syl2id, f, ensure_ascii=False, indent=2)

    @classmethod
    def load(cls, path: str) -> "PinyinTokenizer":
        tok = cls.__new__(cls)
        with open(path, "r", encoding="utf-8") as f:
            tok.syl2id = json.load(f)
        tok.id2syl = {v: k for k, v in tok.syl2id.items()}
        return tok


class CharTokenizer:
    """汉字字符编码器 — 基于语料频率构建"""

    def __init__(self, char_file: Optional[str] = None, corpus_path: Optional[str] = None):
        self.char2id: Dict[str, int] = {}
        self.id2char: Dict[int, str] = {}
        if char_file and os.path.exists(char_file):
            self._load_from_file(char_file)
        elif corpus_path and os.path.exists(corpus_path):
            self._build_from_corpus(corpus_path)
        else:
            self._build_default()

    def _build_from_corpus(self, corpus_path: str, max_chars: int = 8000):
        """从语料文件构建字频排序的词表"""
        print(f"[Tokenizer] 从语料构建字表: {corpus_path}")
        counter = Counter()
        with open(corpus_path, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if "\t" not in line:
                    continue
                _, hanzi = line.split("\t", 1)
                for ch in hanzi:
                    if '\u4e00' <= ch <= '\u9fff':
                        counter[ch] += 1

        # 特殊 token
        for i, tok in enumerate(SPECIAL_TOKENS):
            self.char2id[tok] = i
            self.id2char[i] = tok

        # 按频率降序添加
        offset = len(SPECIAL_TOKENS)
        for idx, (ch, cnt) in enumerate(counter.most_common(max_chars)):
            i = offset + idx
            self.char2id[ch] = i
            self.id2char[i] = ch

        # 常用标点
        punct_offset = offset + len(counter.most_common(max_chars))
        for pi, punct in enumerate("，。、！？：；""''（）《》【】…—"):
            if punct not in self.char2id:
                self.char2id[punct] = punct_offset + pi
                self.id2char[punct_offset + pi] = punct

        print(f"[Tokenizer] 从语料提取 {len(self.char2id) - len(SPECIAL_TOKENS)} 个字符")
        print(f"[Tokenizer] 前10高频: {''.join(ch for ch, _ in counter.most_common(10))}")

    def _build_default(self):
        """默认: 常用简体字 (按频率排序的前 3500 常用字)"""
        # GB2312 一级汉字 3755 个（按拼音+笔画排序，覆盖 99.7% 日常用字）
        # 这里按频率排列前 500 个最常用简体字
        TOP_500 = (
            "的一是不了在人有我他这个们中来上大为和国地到以说时"
            "要就出会也你对生能而子那得于着下自之年过发后作里用"
            "道行所然家种事成方多经么去法学如都同现当没动面起看"
            "定天分还进好小部其些主样理心她本前开但因只从想实日"
            "军者意无力它与长把机十民第公此已工使情明性知全三又"
            "关点正业外将两高间由问很最也重新回把门体别立代头入"
            "气已等做老被保正之白向所教通更将义相望期文几起应合"
            "许手加条特内信号达常表系场加决水手已化更己求制各比"
            "目己第员等直象其平走至设张反结解边界活命步指五少次"
            "品取消认治提计果则务处管边走世身确斯名吃记路术及干"
            "总单史确联受际基色见报局太根改准半空山西数件运什量"
            "位且共感备政反影存任接难线示阿光海达八东京识格深论"
            "言权较吗近却严清百思红花村回写持程风争强往领组首观"
            "落价满调容调易听团众神况构图参眼商转角谈传集双收音"
            "往断飞原古车令按语春绝派费怎望级调细复备拿温科举状"
            "落导场局极留跑低研据选类似局南兵器广县整形石陈唐足"
        )

        for i, tok in enumerate(SPECIAL_TOKENS):
            self.char2id[tok] = i
            self.id2char[i] = tok

        offset = len(SPECIAL_TOKENS)
        added = set()
        idx = 0
        for ch in TOP_500:
            if ch not in added:
                i = offset + idx
                self.char2id[ch] = i
                self.id2char[i] = ch
                added.add(ch)
                idx += 1

        # 常用标点
        for punct in "，。、！？：；""''（）《》【】…—":
            if punct not in self.char2id:
                i = offset + idx
                self.char2id[punct] = i
                self.id2char[i] = punct
                idx += 1

        print(f"[Tokenizer] 默认字表: {len(self.char2id)} 字符（无语料时使用）")

    def _load_from_file(self, path: str):
        with open(path, "r", encoding="utf-8") as f:
            self.char2id = json.load(f)
        self.id2char = {v: k for k, v in self.char2id.items()}

    @property
    def vocab_size(self) -> int:
        return len(self.char2id)

    def encode(self, text: str) -> List[int]:
        """汉字字符串 → token ID 列表"""
        return [self.char2id.get(ch, UNK_ID) for ch in text]

    def decode(self, ids: List[int]) -> str:
        return "".join(self.id2char.get(i, "?") for i in ids)

    def save(self, path: str):
        with open(path, "w", encoding="utf-8") as f:
            json.dump(self.char2id, f, ensure_ascii=False, indent=2)

    @classmethod
    def load(cls, path: str) -> "CharTokenizer":
        tok = cls.__new__(cls)
        tok._load_from_file(path)
        return tok


def build_and_save_vocabs(out_dir: str, corpus_path: str = None):
    """构建并保存词表文件"""
    os.makedirs(out_dir, exist_ok=True)

    py_tok = PinyinTokenizer()
    py_tok.save(os.path.join(out_dir, "pinyin2id.json"))
    print(f"[Tokenizer] 拼音词表: {py_tok.vocab_size} tokens → pinyin2id.json")

    ch_tok = CharTokenizer(corpus_path=corpus_path)
    ch_tok.save(os.path.join(out_dir, "char2id.json"))
    print(f"[Tokenizer] 汉字词表: {ch_tok.vocab_size} tokens → char2id.json")

    # 保存元信息
    meta = {
        "pinyin_vocab_size": py_tok.vocab_size,
        "char_vocab_size": ch_tok.vocab_size,
        "special_tokens": {t: i for i, t in enumerate(SPECIAL_TOKENS)},
        "max_pinyin_len": 32,
        "max_context_len": 64,
    }
    with open(os.path.join(out_dir, "vocab_meta.json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, ensure_ascii=False, indent=2)
    print(f"[Tokenizer] 元信息 → vocab_meta.json")

    return py_tok, ch_tok


if __name__ == "__main__":
    import sys
    corpus = sys.argv[1] if len(sys.argv) > 1 else None
    py_tok, ch_tok = build_and_save_vocabs("trainer/vocab", corpus_path=corpus)

    # 测试
    print("\n=== 测试 ===")
    ids = py_tok.encode(["ni", "hao", "shi", "jie"])
    print(f"拼音 ['ni','hao','shi','jie'] → {ids}")
    print(f"解码 → {py_tok.decode(ids)}")

    ids2 = ch_tok.encode("你好世界")
    print(f"汉字 '你好世界' → {ids2}")
    print(f"解码 → {ch_tok.decode(ids2)}")

    # 覆盖率测试
    if corpus:
        total = 0
        matched = 0
        with open(corpus, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if "\t" not in line:
                    continue
                _, hanzi = line.split("\t", 1)
                for ch in hanzi:
                    if '\u4e00' <= ch <= '\u9fff':
                        total += 1
                        if ch in ch_tok.char2id:
                            matched += 1
        print(f"\n语料覆盖率: {matched}/{total} = {100*matched/max(total,1):.1f}%")
