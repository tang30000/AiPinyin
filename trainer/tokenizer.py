"""
AiPinyin Tokenizer — 拼音/汉字双向编码器

职责:
  1. 拼音音节 → token ID  (约 415 个)
  2. 汉字字符 → token ID  (约 7000 个)
  3. 提供训练数据的编码/解码接口

特殊 Token:
  0 = PAD, 1 = UNK, 2 = BOS, 3 = EOS
"""

import json
import os
from typing import List, Dict, Tuple, Optional

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
        for i, syl in enumerate(PINYIN_SYLLABLES):
            idx = offset + i
            self.syl2id[syl] = idx
            self.id2syl[idx] = syl

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
    """汉字字符编码器 (7000 常用字)"""

    # GB2312 一级汉字 3755 + 二级汉字 3008 + 常用标点 ≈ 7000
    def __init__(self, char_file: Optional[str] = None):
        self.char2id: Dict[str, int] = {}
        self.id2char: Dict[int, str] = {}
        if char_file and os.path.exists(char_file):
            self._load_from_file(char_file)
        else:
            self._build_default()

    def _build_default(self):
        """从 Unicode CJK 基本区构建 7000 常用字表"""
        for i, tok in enumerate(SPECIAL_TOKENS):
            self.char2id[tok] = i
            self.id2char[i] = tok

        offset = len(SPECIAL_TOKENS)
        # 常用汉字范围: U+4E00 - U+9FFF
        # 取前 7000 个最常用的（按 Unicode 排列）
        # 实际项目中应按频率排序，这里先用 Unicode 顺序占位
        count = 0
        for cp in range(0x4E00, 0x9FFF + 1):
            ch = chr(cp)
            idx = offset + count
            self.char2id[ch] = idx
            self.id2char[idx] = ch
            count += 1
            if count >= 7000:
                break

        # 常用标点
        for punct in "，。、！？：；""''（）《》【】…—":
            if punct not in self.char2id:
                idx = offset + count
                self.char2id[punct] = idx
                self.id2char[idx] = punct
                count += 1

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


def build_and_save_vocabs(out_dir: str):
    """构建并保存词表文件"""
    os.makedirs(out_dir, exist_ok=True)

    py_tok = PinyinTokenizer()
    py_tok.save(os.path.join(out_dir, "pinyin2id.json"))
    print(f"[Tokenizer] 拼音词表: {py_tok.vocab_size} tokens → pinyin2id.json")

    ch_tok = CharTokenizer()
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
    py_tok, ch_tok = build_and_save_vocabs("trainer/vocab")

    # 测试
    print("\n=== 测试 ===")
    ids = py_tok.encode(["ni", "hao", "shi", "jie"])
    print(f"拼音 ['ni','hao','shi','jie'] → {ids}")
    print(f"解码 → {py_tok.decode(ids)}")

    ids2 = ch_tok.encode("你好世界")
    print(f"汉字 '你好世界' → {ids2}")
    print(f"解码 → {ch_tok.decode(ids2)}")
