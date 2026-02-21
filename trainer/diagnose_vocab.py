"""诊断 char2id 和语料的匹配问题"""
import json
import os

# 加载 char2id
vocab_dir = os.path.join(os.path.dirname(__file__), "vocab")
with open(os.path.join(vocab_dir, "char2id.json"), "r", encoding="utf-8") as f:
    char2id = json.load(f)

# 去掉特殊 token
real_chars = {k: v for k, v in char2id.items() if not k.startswith("<")}
print(f"char2id 总字符: {len(real_chars)}")
print(f"前20个字符: {''.join(list(real_chars.keys())[:20])}")
print(f"是否含'的': {'的' in real_chars}")
print(f"是否含'我': {'我' in real_chars}")
print(f"是否含'你': {'你' in real_chars}")
print(f"是否含'是': {'是' in real_chars}")
print(f"是否含'了': {'了' in real_chars}")
print(f"是否含'不': {'不' in real_chars}")

# 检查语料覆盖率
corpus_path = os.path.join(os.path.dirname(__file__), "data", "corpus.txt")
if os.path.exists(corpus_path):
    corpus_chars = set()
    total = 0
    matched = 0
    with open(corpus_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if "\t" not in line:
                continue
            _, hanzi = line.split("\t", 1)
            for ch in hanzi:
                if '\u4e00' <= ch <= '\u9fff':
                    corpus_chars.add(ch)
                    total += 1
                    if ch in real_chars:
                        matched += 1

    print(f"\n语料中独立汉字: {len(corpus_chars)}")
    print(f"语料中总汉字数: {total:,}")
    print(f"在 char2id 中匹配: {matched:,} ({100*matched/max(total,1):.1f}%)")
    print(f"未匹配(UNK): {total - matched:,} ({100*(total-matched)/max(total,1):.1f}%)")
    
    # 找出最常见的未匹配字
    from collections import Counter
    unk_counter = Counter()
    with open(corpus_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if "\t" not in line:
                continue
            _, hanzi = line.split("\t", 1)
            for ch in hanzi:
                if '\u4e00' <= ch <= '\u9fff' and ch not in real_chars:
                    unk_counter[ch] += 1
    
    print(f"\n最常见的 UNK 字 (前30):")
    for ch, cnt in unk_counter.most_common(30):
        print(f"  {ch} (U+{ord(ch):04X}): {cnt} 次")
else:
    print("\n语料文件不存在")
