# -*- coding: utf-8 -*-
"""
用 jieba 词频表自动校对补充 dict.txt
jieba 内置 ~35万条中文词频数据, 覆盖日常用词
"""
import os, json

# jieba 词频文件自带在安装包里
try:
    import jieba
    jieba_dict = os.path.join(os.path.dirname(jieba.__file__), 'dict.txt')
    print(f"jieba 词典: {jieba_dict}")
except ImportError:
    print("请先安装 jieba: pip install jieba")
    exit(1)

BASE = os.path.dirname(os.path.dirname(__file__))
OUR_DICT = os.path.join(BASE, 'target', 'debug', 'dict.txt')
OUR_DICT2 = os.path.join(BASE, 'dict.txt')
P2C_PATH = os.path.join(BASE, 'target', 'debug', 'pinyin2char.json')

# 加载 pinyin2char (用于生成拼音)
p2c = json.load(open(P2C_PATH, 'r', encoding='utf-8'))
# 反向映射: char → pinyin (只接受含韵母的拼音, 排除纯声母如 y/g/z)
c2py = {}
VOWELS = set('aeiouv')
for py, chars in p2c.items():
    if not any(c in VOWELS for c in py):
        continue  # 跳过纯声母 (y, g, z, w, ...)
    for c in chars:
        if c not in c2py:
            c2py[c] = py

def word_pinyin(word):
    """把汉字词组转为拼音拼接 (如 '速度' → 'sudu')"""
    pys = []
    for c in word:
        if c in c2py:
            pys.append(c2py[c])
        else:
            return None  # 有字不在拼音表中
    return ''.join(pys)

# ── 读取 jieba 词频 ──
print("读取 jieba 词频...")
jieba_words = {}  # pinyin → [(word, freq), ...]
jieba_count = 0
with open(jieba_dict, 'r', encoding='utf-8') as f:
    for line in f:
        parts = line.strip().split()
        if len(parts) >= 2:
            word = parts[0]
            try:
                freq = int(parts[1])
            except ValueError:
                continue
            # 只要 2-6 字的词
            if 2 <= len(word) <= 6 and freq >= 50:
                py = word_pinyin(word)
                if py:
                    jieba_words.setdefault(py, []).append((word, freq))
                    jieba_count += 1

print(f"jieba 有效词条: {jieba_count}")

# ── 读取我们的字典 ──
print("读取当前字典...")
our_entries = {}  # (pinyin, word) → weight
with open(OUR_DICT, 'r', encoding='utf-8') as f:
    for line in f:
        parts = line.strip().split(',', 2)
        if len(parts) >= 3:
            py, word = parts[0].strip(), parts[1].strip()
            try:
                w = int(parts[2].strip())
            except ValueError:
                w = 100
            our_entries[(py, word)] = w

print(f"当前字典条目: {len(our_entries)}")

# ── 对比找出缺失 + 权重偏低的词 ──
missing = []
low_weight = []

for py, entries in jieba_words.items():
    for word, jieba_freq in entries:
        key = (py, word)
        if key not in our_entries:
            # 字典中完全缺失
            # jieba freq → 我们的权重: 映射到 500-900 区间
            if jieba_freq >= 10000:
                w = 900
            elif jieba_freq >= 5000:
                w = 850
            elif jieba_freq >= 1000:
                w = 800
            elif jieba_freq >= 500:
                w = 700
            elif jieba_freq >= 100:
                w = 600
            else:
                w = 500
            missing.append((py, word, w, jieba_freq))
        else:
            # 字典中存在但权重可能过低
            our_w = our_entries[key]
            if jieba_freq >= 5000 and our_w < 800:
                # 高频词权重太低
                new_w = max(our_w, 850)
                low_weight.append((py, word, our_w, new_w, jieba_freq))
            elif jieba_freq >= 1000 and our_w < 500:
                new_w = max(our_w, 700)
                low_weight.append((py, word, our_w, new_w, jieba_freq))

print(f"\n缺失词: {len(missing)}")
print(f"权重偏低: {len(low_weight)}")

# 显示示例
print("\n缺失词示例 (top-20 by jieba freq):")
missing.sort(key=lambda x: -x[3])
for py, word, w, jf in missing[:20]:
    print(f"  {py},{word},{w}  (jieba freq={jf})")

print("\n权重偏低示例 (top-20):")
low_weight.sort(key=lambda x: -x[4])
for py, word, old_w, new_w, jf in low_weight[:20]:
    print(f"  {py},{word}: {old_w}→{new_w}  (jieba={jf})")

# ── 应用修复 ──
print(f"\n应用修复...")

for path in [OUR_DICT, OUR_DICT2]:
    if not os.path.exists(path):
        continue
    
    # 读取所有行
    with open(path, 'r', encoding='utf-8') as f:
        lines = f.readlines()
    
    # 构建提权映射
    boost_map = {(py, word): new_w for py, word, old_w, new_w, jf in low_weight}
    
    out = []
    boosted = 0
    for line in lines:
        parts = line.strip().split(',', 2)
        if len(parts) >= 3:
            py, word = parts[0].strip(), parts[1].strip()
            key = (py, word)
            if key in boost_map:
                try:
                    old_w = int(parts[2].strip())
                    if boost_map[key] > old_w:
                        line = f"{py},{word},{boost_map[key]}\n"
                        boosted += 1
                except ValueError:
                    pass
        out.append(line if line.endswith('\n') else line + '\n')
    
    # 添加缺失词
    added = 0
    existing = set()
    for line in out:
        parts = line.strip().split(',', 2)
        if len(parts) >= 2:
            existing.add((parts[0].strip(), parts[1].strip()))
    
    for py, word, w, jf in missing:
        if (py, word) not in existing:
            out.append(f"{py},{word},{w}\n")
            added += 1
    
    with open(path, 'w', encoding='utf-8') as f:
        f.writelines(out)
    
    print(f"  {path}: +{added} 新词, ↑{boosted} 提权")

print("\n✅ 完成!")
