# -*- coding: utf-8 -*-
"""
从 Duyu/Pinyin-Hanzi 语料生成高质量词典

输入: trainer/data/pinyin2hanzi.csv (Duyu语料)
输出: dict.txt (拼音,汉字,词频)

与 convert_dict.py 不同:
- 直接从 Duyu 155万条真实数据统计词频
- 覆盖更全面: 单字 + 2字词 + 3-6字词 + 长词组
- 词频基于真实出现次数

用法:
  python tools/build_dict.py
"""

import os
import re
import csv
from collections import Counter


def remove_tone(pinyin_str: str) -> str:
    """去掉拼音中的声调数字, 例如 'lan2 diao4' → 'lan diao'"""
    return re.sub(r'([a-z]+)[1-5]', r'\\1', pinyin_str)


def build_dict(csv_path: str, output_path: str, min_freq: int = 1):
    """从 CSV 构建词典"""

    # 统计: (连写拼音, 汉字) → 出现次数
    word_freq = Counter()
    total = 0
    skipped = 0

    print(f"读取 {csv_path} ...")

    with open(csv_path, 'r', encoding='utf-8') as f:
        reader = csv.reader(f)
        for row in reader:
            if len(row) < 2:
                skipped += 1
                continue

            hanzi = row[0].strip()
            pinyin_toned = row[1].strip()

            # 去声调
            pinyin_toneless = remove_tone(pinyin_toned)
            syllables = pinyin_toneless.split()

            # 提取纯汉字
            pure_hanzi = ''.join(ch for ch in hanzi if '\u4e00' <= ch <= '\u9fff')

            if len(syllables) != len(pure_hanzi) or len(syllables) == 0:
                skipped += 1
                continue

            # 验证拼音
            valid = True
            for syl in syllables:
                if not syl.isalpha() or len(syl) > 6:
                    valid = False
                    break
            if not valid:
                skipped += 1
                continue

            # 连写拼音
            pinyin_joined = ''.join(syllables)

            # 整词统计
            word_freq[(pinyin_joined, pure_hanzi)] += 1

            # 如果词长 > 1, 也统计每个单字
            if len(pure_hanzi) > 1:
                for syl, ch in zip(syllables, pure_hanzi):
                    word_freq[(syl, ch)] += 1

            total += 1
            if total % 500000 == 0:
                print(f"  已处理 {total:,} ...")

    print(f"\n统计: 有效 {total:,}, 跳过 {skipped:,}")
    print(f"唯一词条: {len(word_freq):,}")

    # 按词频排序
    sorted_entries = sorted(word_freq.items(), key=lambda x: -x[1])

    # 过滤低频词
    entries = [(py, hz, freq) for (py, hz), freq in sorted_entries if freq >= min_freq]
    print(f"过滤后 (freq>={min_freq}): {len(entries):,} 条")

    # 写出
    os.makedirs(os.path.dirname(output_path) or '.', exist_ok=True)
    with open(output_path, 'w', encoding='utf-8') as f:
        f.write('# AiPinyin 词典 — 自动从 Duyu 语料生成\n')
        f.write('# 格式: 拼音,汉字,权重(词频)\n')
        f.write(f'# 共 {len(entries)} 条\n')
        for py, hz, freq in entries:
            f.write(f'{py},{hz},{freq}\n')

    # 统计信息
    single_chars = sum(1 for _, hz, _ in entries if len(hz) == 1)
    two_chars = sum(1 for _, hz, _ in entries if len(hz) == 2)
    multi_chars = sum(1 for _, hz, _ in entries if len(hz) > 2)

    print(f"\n✅ 输出: {output_path}")
    print(f"   总词条: {len(entries):,}")
    print(f"   单字: {single_chars:,}")
    print(f"   双字词: {two_chars:,}")
    print(f"   多字词: {multi_chars:,}")

    # 显示 top 词
    print(f"\n词频 Top 20:")
    for py, hz, freq in entries[:20]:
        print(f"  {py} → {hz}  (freq={freq})")


if __name__ == '__main__':
    import sys
    data_dir = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'trainer', 'data')
    csv_path = os.path.join(data_dir, 'pinyin2hanzi.csv')
    output_path = sys.argv[1] if len(sys.argv) > 1 else 'dict.txt'
    min_freq = int(sys.argv[2]) if len(sys.argv) > 2 else 2

    if not os.path.exists(csv_path):
        print(f"❌ 找不到 {csv_path}")
        print(f"请先运行: python trainer/download_dataset.py")
        exit(1)

    build_dict(csv_path, output_path, min_freq=min_freq)
