# -*- coding: utf-8 -*-
"""
用 pypinyin 把 jieba 词频精确映射到 dict.txt
解决之前 c2py 多音字映射错误的问题
"""
import os, json

BASE = os.path.dirname(os.path.dirname(__file__))
OUT = os.path.join(BASE, 'target', 'debug')
DICT_PATHS = [os.path.join(OUT, 'dict.txt'), os.path.join(BASE, 'dict.txt')]

# 安装 pypinyin
try:
    from pypinyin import pinyin, Style
except ImportError:
    print("安装 pypinyin...")
    os.system('pip install pypinyin -q')
    from pypinyin import pinyin, Style

import jieba

def word_to_pinyin(word):
    """用 pypinyin 获取准确拼音 (处理多音字)"""
    try:
        pys = pinyin(word, style=Style.NORMAL, heteronym=False)
        return ''.join([p[0] for p in pys])
    except:
        return None

# 加载 jieba 词频
jieba_dict = os.path.join(os.path.dirname(jieba.__file__), 'dict.txt')
print(f"jieba 词典: {jieba_dict}")

jieba_freq = {}  # (pinyin, word) → freq
count = 0
with open(jieba_dict, 'r', encoding='utf-8') as f:
    for line in f:
        parts = line.strip().split()
        if len(parts) >= 2:
            word = parts[0]
            try: freq = int(parts[1])
            except: continue
            if 2 <= len(word) <= 6 and freq >= 10:
                py = word_to_pinyin(word)
                if py:
                    jieba_freq[(py, word)] = freq
                    count += 1
                    if count % 5000 == 0:
                        print(f"  已处理 {count} 词...")

print(f"jieba 有效词条: {count}")

# 映射 jieba 频率 → 字典权重
def freq_to_weight(freq):
    """jieba 频率 → 字典权重 (100-950)"""
    if freq >= 50000: return 950
    if freq >= 20000: return 920
    if freq >= 10000: return 900
    if freq >= 5000:  return 850
    if freq >= 2000:  return 800
    if freq >= 1000:  return 750
    if freq >= 500:   return 700
    if freq >= 200:   return 600
    if freq >= 50:    return 500
    return 400

# 处理每个字典文件
for dict_path in DICT_PATHS:
    if not os.path.exists(dict_path):
        continue
    
    print(f"\n处理: {dict_path}")
    
    # 读取现有条目
    lines = []
    existing = {}  # (py, word) → (line_idx, weight)
    with open(dict_path, 'r', encoding='utf-8') as f:
        for i, line in enumerate(f):
            lines.append(line)
            parts = line.strip().split(',', 2)
            if len(parts) >= 3:
                py, word = parts[0].strip(), parts[1].strip()
                try: w = int(parts[2].strip())
                except: w = 100
                existing[(py, word)] = (i, w)
    
    boosted = 0
    added = 0
    
    # 提权: 已存在但权重偏低的词
    for (py, word), freq in jieba_freq.items():
        target_w = freq_to_weight(freq)
        key = (py, word)
        if key in existing:
            idx, cur_w = existing[key]
            if target_w > cur_w:
                lines[idx] = f"{py},{word},{target_w}\n"
                boosted += 1
        else:
            # 新增
            lines.append(f"{py},{word},{target_w}\n")
            added += 1
    
    with open(dict_path, 'w', encoding='utf-8') as f:
        f.writelines(lines)
    
    print(f"  ↑{boosted} 提权, +{added} 新词")

# 验证关键词
print("\n验证关键词:")
for dict_path in DICT_PATHS[:1]:
    checks = ["dazi,打字", "guji,估计", "ruguo,如果", "sudu,速度", "keyi,可以", 
              "jintian,今天", "zhichi,支持", "dechu,得出", "zenme,怎么"]
    for check in checks:
        py, word = check.split(',')
        with open(dict_path, 'r', encoding='utf-8') as f:
            for line in f:
                if line.startswith(f"{py},{word},"):
                    print(f"  {line.strip()}")
                    break

print("\n✅ 完成!")
