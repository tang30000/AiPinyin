# -*- coding: utf-8 -*-
"""
补充 "的/了/是/在/和/不/也/都/就/会/能/要/让/把/被/给/对/向/从" 等
虚词与常用字组成的高频组合词

这些词不在传统词库里, 但输入法分词必须识别它们
"""
import os, json

BASE = os.path.dirname(os.path.dirname(__file__))
P2C_PATH = os.path.join(BASE, 'target', 'debug', 'pinyin2char.json')
DICT_PATHS = [
    os.path.join(BASE, 'target', 'debug', 'dict.txt'),
    os.path.join(BASE, 'dict.txt'),
]

try:
    from pypinyin import pinyin, Style
except ImportError:
    os.system('pip install pypinyin -q')
    from pypinyin import pinyin, Style

def char_pinyin(ch):
    try:
        return pinyin(ch, style=Style.NORMAL)[0][0]
    except:
        return None

# 从字典自动提取所有常用单字 (权重>=400)
# 不再手动列举, 确保覆盖全面
COMMON_CHARS = set()
for dict_path in DICT_PATHS:
    if not os.path.exists(dict_path):
        continue
    with open(dict_path, 'r', encoding='utf-8') as f:
        for line in f:
            parts = line.strip().split(',', 2)
            if len(parts) >= 3:
                word = parts[1].strip()
                try: w = int(parts[2].strip())
                except: continue
                if len(word) == 1 and w >= 400:
                    COMMON_CHARS.add(word)
    break  # 只读第一个

# 补充一些常用但权重可能不高的字
COMMON_CHARS.update(list(
    "我你他她它们这那哪谁什么怎为有没到过来去做看说想"
    "好坏大小多少新旧高低长短远近快慢早晚前后上下左右"
    "吃喝玩睡走跑站坐开关买卖读写听讲打拿放送接收"
    "红黄蓝绿白黑强弱美丑对错真假难易"
))

print(f"常用单字: {len(COMMON_CHARS)} 个")

# 虚词后缀
SUFFIXES = {
    "的": 850,  # 新的 好的 我的
    "了": 800,  # 好了 来了 走了
    "是": 800,  # 就是 不是 都是
    "在": 750,  # 还在 现在 正在
    "和": 700,  # 你和 我和
    "也": 750,  # 我也 他也 都也
    "都": 750,  # 我都 他都
    "就": 750,  # 我就 他就
    "会": 750,  # 我会 他会
    "能": 750,  # 我能 他能
    "要": 750,  # 我要 他要
    "不": 800,  # 好不 是不
    "很": 750,  # 我很 他很
    "太": 700,  # 我太 他太
    "最": 700,  # 我最 他最
    "还": 750,  # 我还 他还
    "又": 700,  # 我又 他又
    "再": 700,  # 我再 他再
    "去": 750,  # 我去 你去
    "来": 750,  # 我来 你来
    "过": 750,  # 来过 去过
    "着": 700,  # 看着 吃着
    "得": 750,  # 跑得 好得 
}

# 常用前缀词组
PREFIXES = {
    "不": 800,  # 不好 不是 不要
    "没": 800,  # 没有 没来
    "很": 750,  # 很好 很大
    "太": 700,  # 太好 太大
    "最": 700,  # 最好 最大
    "都": 750,  # 都是 都有
    "就": 750,  # 就是 就要
    "还": 750,  # 还是 还有 还好
    "也": 750,  # 也是 也有
    "可": 700,  # 可以 可能 可是
}

COMMON_SUFFIX_CHARS = list(
    "好坏大小多少长短高低快慢远近新旧强弱对错美丑"
    "行想说看来去做走有是要能会知道"
)

new_words = set()

# 生成 char+suffix 组合
for ch in COMMON_CHARS:
    ch_py = char_pinyin(ch)
    if not ch_py:
        continue
    for suffix, weight in SUFFIXES.items():
        s_py = char_pinyin(suffix)
        if not s_py:
            continue
        word = ch + suffix
        word_py = ch_py + s_py
        new_words.add((word_py, word, weight))

# 生成 prefix+char 组合
for prefix, weight in PREFIXES.items():
    p_py = char_pinyin(prefix)
    if not p_py:
        continue
    for ch in COMMON_SUFFIX_CHARS:
        ch_py = char_pinyin(ch)
        if not ch_py:
            continue
        word = prefix + ch
        word_py = p_py + ch_py
        new_words.add((word_py, word, weight))

print(f"生成虚词组合: {len(new_words)} 条")

# 写入字典
for dict_path in DICT_PATHS:
    if not os.path.exists(dict_path):
        continue
    
    # 读取现有条目
    existing = set()
    with open(dict_path, 'r', encoding='utf-8') as f:
        for line in f:
            parts = line.strip().split(',', 2)
            if len(parts) >= 2:
                existing.add((parts[0].strip(), parts[1].strip()))
    
    added = 0
    with open(dict_path, 'a', encoding='utf-8') as f:
        for py, word, weight in sorted(new_words):
            if (py, word) not in existing:
                f.write(f"{py},{word},{weight}\n")
                added += 1
    
    print(f"  {dict_path}: +{added} 新词")

# 验证
print("\n验证:")
for check in ["wode,我的", "nide,你的", "tade,他的", "xinde,新的", 
              "haode,好的", "bushi,不是", "meiyou,没有", "haile,还了",
              "jiude,旧的", "dade,大的", "duide,对的", "buhao,不好"]:
    py, word = check.split(',')
    found = False
    for dict_path in DICT_PATHS[:1]:
        with open(dict_path, 'r', encoding='utf-8') as f:
            for line in f:
                if line.startswith(f"{py},{word},"):
                    print(f"  ✅ {line.strip()}")
                    found = True
                    break
    if not found:
        print(f"  ❌ {check}")

print("\n✅ 完成!")
