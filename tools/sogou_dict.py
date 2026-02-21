# -*- coding: utf-8 -*-
"""
搜狗 .scel 细胞词库解析 + 下载 + 合并到 dict.txt

功能:
  1. 下载搜狗热门词库 (.scel)
  2. 解析二进制 scel 格式
  3. 合并到现有 dict.txt

用法:
  python tools/sogou_dict.py                    # 下载热门词库并合并
  python tools/sogou_dict.py path/to/file.scel  # 解析单个 scel 文件
"""

import struct
import os
import sys
import urllib.request

# ── 搜狗热门词库 ID (可扩展) ──
# 格式: (词库ID, 名称)
POPULAR_DICTS = [
    (15117, "网络流行新词"),
    (4, "城市信息大全"),
    (15128, "日常用语大词库"),
    (11640, "古诗词"),
    (4851, "成语大全"),
    (13544, "IT计算机词汇大全"),
    (1, "城市词库"),
    (75, "游戏词汇大全"),
    (150, "食物大全"),
    (180, "医学词汇大全"),
    (4335, "人名大全"),
    (19341, "常用词库"),
]


def parse_scel(file_path):
    """
    解析搜狗 .scel 文件，返回 [(拼音连写, 汉字)] 列表
    """
    with open(file_path, 'rb') as f:
        data = f.read()

    if len(data) < 0x1540:
        print(f"  ⚠ 文件太小，跳过: {file_path}")
        return []

    # ── 解析拼音表 (0x1540: uint32 count, 0x1544 开始条目) ──
    pinyin_table = {}
    pinyin_count = struct.unpack_from('<I', data, 0x1540)[0]
    pos = 0x1544

    for _ in range(min(pinyin_count, 500)):
        if pos + 4 > len(data):
            break
        idx = struct.unpack_from('<H', data, pos)[0]
        length = struct.unpack_from('<H', data, pos + 2)[0]
        pos += 4
        if length > 20 or pos + length > len(data):
            break
        try:
            pinyin = data[pos:pos + length].decode('UTF-16LE')
            pinyin_table[idx] = pinyin
        except:
            pass
        pos += length

    # ── 词组表紧跟拼音表之后 ──
    word_table_start = pos
    results = []
    pos = word_table_start

    while pos < len(data) - 4:
        if pos + 4 > len(data):
            break

        # 同音词数量 + 拼音索引区字节长度
        word_count = struct.unpack_from('<H', data, pos)[0]
        py_bytes_len = struct.unpack_from('<H', data, pos + 2)[0]
        pos += 4

        if word_count == 0 or word_count > 1000 or py_bytes_len > 100:
            break

        # 读取拼音索引 (每个 2 字节)
        if pos + py_bytes_len > len(data):
            break

        pinyin_parts = []
        py_count = py_bytes_len // 2
        for i in range(py_count):
            py_idx = struct.unpack_from('<H', data, pos + i * 2)[0]
            if py_idx in pinyin_table:
                pinyin_parts.append(pinyin_table[py_idx])
        pos += py_bytes_len

        pinyin_joined = ''.join(pinyin_parts).lower()

        # 读取同音词
        for _ in range(word_count):
            if pos + 2 > len(data):
                break
            word_len = struct.unpack_from('<H', data, pos)[0]
            pos += 2

            if word_len > 200 or pos + word_len > len(data):
                break
            try:
                word = data[pos:pos + word_len].decode('UTF-16LE')
                if pinyin_joined and word:
                    results.append((pinyin_joined, word))
            except:
                pass
            pos += word_len

            # 扩展信息 (通常 12 字节: 2 字节长度 + 10 字节数据)
            if pos + 2 > len(data):
                break
            ext_len = struct.unpack_from('<H', data, pos)[0]
            pos += 2
            if ext_len > 100 or pos + ext_len > len(data):
                break
            pos += ext_len

    return results


def download_scel(dict_id, name, output_dir):
    """下载搜狗词库 .scel 文件"""
    import urllib.parse
    encoded_name = urllib.parse.quote(name)
    url = f"https://pinyin.sogou.com/d/dict/download_cell.php?id={dict_id}&name={encoded_name}"
    output_path = os.path.join(output_dir, f"sogou_{dict_id}.scel")

    if os.path.exists(output_path):
        print(f"  ✓ 已存在: {name}")
        return output_path

    print(f"  ⬇ 下载: {name} (id={dict_id})...")
    try:
        req = urllib.request.Request(url, headers={
            'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)',
            'Referer': 'https://pinyin.sogou.com/dict/',
        })
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = resp.read()
            with open(output_path, 'wb') as f:
                f.write(data)
        print(f"    → {len(data)} bytes")
        return output_path
    except Exception as e:
        print(f"    ✗ 下载失败: {e}")
        return None


def merge_to_dict(sogou_entries, existing_dict_path, output_path):
    """将搜狗词条合并到现有 dict.txt"""
    # 读取现有词典
    existing = {}  # (pinyin, word) -> weight
    if os.path.exists(existing_dict_path):
        with open(existing_dict_path, 'r', encoding='utf-8') as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith('#'):
                    continue
                parts = line.split(',', 2)
                if len(parts) >= 3:
                    py, word = parts[0], parts[1]
                    weight = int(parts[2]) if parts[2].isdigit() else 50
                    key = (py, word)
                    if key not in existing or weight > existing[key]:
                        existing[key] = weight

    # 合并搜狗词条 (新词权重 50, 已有不覆盖)
    added = 0
    for pinyin, word in sogou_entries:
        # 过滤非法
        if not pinyin.isalpha() or not word:
            continue
        key = (pinyin, word)
        if key not in existing:
            existing[key] = 50
            added += 1

    # 排序输出
    sorted_entries = sorted(existing.items(), key=lambda x: -x[1])

    with open(output_path, 'w', encoding='utf-8') as f:
        f.write('# AiPinyin 词典 — 自动生成 (含搜狗词库)\n')
        f.write(f'# 共 {len(sorted_entries)} 条\n')
        for (py, word), weight in sorted_entries:
            f.write(f'{py},{word},{weight}\n')

    print(f"\n✅ 合并完成: 新增 {added} 条, 总计 {len(sorted_entries)} 条 → {output_path}")


def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_dir = os.path.dirname(script_dir)
    scel_dir = os.path.join(project_dir, 'tools', 'scel_cache')
    dict_path = os.path.join(project_dir, 'dict.txt')

    if len(sys.argv) > 1:
        # 解析单个文件
        scel_path = sys.argv[1]
        entries = parse_scel(scel_path)
        print(f"解析 {scel_path}: {len(entries)} 条")
        for py, word in entries[:20]:
            print(f"  {py} → {word}")
        if len(entries) > 20:
            print(f"  ... 共 {len(entries)} 条")
        return

    # 批量下载 + 解析
    os.makedirs(scel_dir, exist_ok=True)

    all_entries = []
    for dict_id, name in POPULAR_DICTS:
        scel_path = download_scel(dict_id, name, scel_dir)
        if scel_path and os.path.exists(scel_path):
            entries = parse_scel(scel_path)
            print(f"    解析: {len(entries)} 条 from {name}")
            all_entries.extend(entries)

    print(f"\n总计从搜狗词库解析: {len(all_entries)} 条")

    # 去重
    unique = list(set(all_entries))
    print(f"去重后: {len(unique)} 条")

    # 合并到 dict.txt
    merge_to_dict(unique, dict_path, dict_path)


if __name__ == '__main__':
    main()
