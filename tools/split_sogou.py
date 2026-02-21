"""将搜狗 scel 词库按类别拆分到 dict/ 目录"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from tools.sogou_dict import parse_scel

# 词库ID → 文件名映射
DICT_MAP = {
    'sogou_15117.scel': 'sogou_neologism',    # 网络流行新词
    'sogou_4.scel':     'sogou_city',          # 城市信息大全
    'sogou_15128.scel': 'sogou_daily',         # 日常用语
    'sogou_11640.scel': 'sogou_poem',          # 古诗词
    'sogou_4851.scel':  'sogou_idiom',         # 成语
    'sogou_13544.scel': 'sogou_it',            # IT计算机
    'sogou_1.scel':     'sogou_region',        # 城市词库
    'sogou_75.scel':    'sogou_game',          # 游戏
    'sogou_150.scel':   'sogou_food',          # 食物
    'sogou_180.scel':   'sogou_medical',       # 医学
    'sogou_4335.scel':  'sogou_names',         # 人名
    'sogou_19341.scel': 'sogou_common',        # 常用词
}

scel_dir = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'tools', 'scel_cache')
dict_dir = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'dict')
os.makedirs(dict_dir, exist_ok=True)

for scel_file, name in DICT_MAP.items():
    path = os.path.join(scel_dir, scel_file)
    if not os.path.exists(path):
        print(f"  ⚠ 缺少 {scel_file}")
        continue

    entries = parse_scel(path)
    if not entries:
        print(f"  ⚠ {name}: 0 条, 跳过")
        continue

    # 去重
    seen = set()
    unique = []
    for py, word in entries:
        if not py.isalpha():
            continue
        key = (py, word)
        if key not in seen:
            seen.add(key)
            unique.append((py, word))

    out_path = os.path.join(dict_dir, f'{name}.txt')
    with open(out_path, 'w', encoding='utf-8') as f:
        f.write(f'# {name} - 搜狗细胞词库\n')
        f.write(f'# 共 {len(unique)} 条\n')
        for py, word in unique:
            f.write(f'{py},{word},50\n')

    print(f"  ✅ {name}.txt: {len(unique)} 条")
