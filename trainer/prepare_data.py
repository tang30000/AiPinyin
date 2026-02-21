"""
将 Duyu/Pinyin-Hanzi CSV 转换为训练格式

输入 CSV 格式: 汉字,拼音 (带声调数字)
  例: 蓝调酒吧,lan2 diao4 jiu3 ba1

输出 corpus.txt 格式: 拼音(无声调)\t汉字
  例: lan diao jiu ba\t蓝调酒吧

用法:
  python trainer/prepare_data.py
"""

import os
import re
import csv
import random

def remove_tone(pinyin_str: str) -> str:
    """去掉拼音中的声调数字, 例如 'lan2 diao4' → 'lan diao'"""
    # 匹配每个音节末尾的数字 1-5
    return re.sub(r'([a-z]+)[1-5]', r'\1', pinyin_str)


def is_valid_sample(hanzi: str, pinyin: str) -> bool:
    """验证样本有效性"""
    syllables = pinyin.strip().split()
    # 过滤掉包含非中文字符的样本（标点除外）
    # 每个音节对应一个汉字
    chars = []
    for ch in hanzi:
        if '\u4e00' <= ch <= '\u9fff':
            chars.append(ch)
        elif ch in '，。、！？：；""''（）《》【】…—':
            # 标点单独不占拼音位
            continue
        else:
            # 含有其他字符(日文、特殊符号等), 跳过
            return False

    return len(syllables) == len(chars) and len(chars) > 0


def process_csv(csv_path: str, out_path: str, max_samples: int = 0):
    """处理 CSV 文件"""
    valid = 0
    skipped = 0
    samples = []

    print(f"读取 {csv_path} ...")

    with open(csv_path, 'r', encoding='utf-8') as f:
        reader = csv.reader(f)
        for row_idx, row in enumerate(reader):
            if len(row) < 2:
                skipped += 1
                continue

            hanzi = row[0].strip()
            pinyin_toned = row[1].strip()

            # 去声调
            pinyin_toneless = remove_tone(pinyin_toned)

            # 清理: 去掉标点符号位置
            # 先简单匹配: 每个拼音音节对应汉字序列中的一个汉字
            syllables = pinyin_toneless.split()
            # 提取纯汉字 (去掉标点)
            pure_hanzi = ''.join(ch for ch in hanzi if '\u4e00' <= ch <= '\u9fff')

            if len(syllables) != len(pure_hanzi) or len(syllables) == 0:
                skipped += 1
                continue

            # 验证每个音节是合法拼音 (基本检查)
            valid_pinyin = True
            for syl in syllables:
                if not syl.isalpha() or len(syl) > 6:
                    valid_pinyin = False
                    break
            if not valid_pinyin:
                skipped += 1
                continue

            samples.append(f"{pinyin_toneless}\t{pure_hanzi}")
            # 首字母版本 (简拼): "ni hao" → "n h"
            initials = ' '.join(s[0] for s in syllables)
            if initials != pinyin_toneless:  # 避免单字母重复
                samples.append(f"{initials}\t{pure_hanzi}")
            valid += 1

            if valid % 200000 == 0:
                print(f"  已处理 {valid:,} 有效样本 ...")

            if max_samples > 0 and valid >= max_samples:
                break

    print(f"\n统计: 有效 {valid:,}, 跳过 {skipped:,}")

    # 随机打乱
    random.seed(42)
    random.shuffle(samples)

    # 写入
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    with open(out_path, 'w', encoding='utf-8') as f:
        f.write('\n'.join(samples) + '\n')

    # 统计信息
    total_chars = sum(len(s.split('\t')[1]) for s in samples)
    avg_len = total_chars / max(len(samples), 1)
    max_len = max((len(s.split('\t')[1]) for s in samples), default=0)

    print(f"\n✅ 输出: {out_path}")
    print(f"   样本数: {len(samples):,}")
    print(f"   总字符数: {total_chars:,}")
    print(f"   平均长度: {avg_len:.1f} 字")
    print(f"   最大长度: {max_len} 字")

    # 显示前几个样本
    print(f"\n前 5 个样本:")
    for s in samples[:5]:
        py, hz = s.split('\t')
        print(f"  {py} → {hz}")


if __name__ == '__main__':
    import sys
    data_dir = os.path.join(os.path.dirname(__file__), 'data')
    csv_path = os.path.join(data_dir, 'pinyin2hanzi.csv')
    out_path = os.path.join(data_dir, 'corpus.txt')

    max_samples = int(sys.argv[1]) if len(sys.argv) > 1 else 0

    if not os.path.exists(csv_path):
        print(f"❌ 找不到 {csv_path}")
        print(f"请先运行: python trainer/download_dataset.py")
        exit(1)

    process_csv(csv_path, out_path, max_samples=max_samples)
