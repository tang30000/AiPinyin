# -*- coding: utf-8 -*-
"""测试: 字典词图 + AI评分 的整句输入"""
import os, json, numpy as np, onnxruntime as ort
os.environ['CUDA_VISIBLE_DEVICES'] = ''
OUT = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'target', 'debug')
s = ort.InferenceSession(os.path.join(OUT, 'weights.onnx'))
py2id = json.load(open(os.path.join(OUT, 'pinyin2id.json'), 'r', encoding='utf-8'))
ch2id = json.load(open(os.path.join(OUT, 'char2id.json'), 'r', encoding='utf-8'))
p2c = json.load(open(os.path.join(OUT, 'pinyin2char.json'), 'r', encoding='utf-8'))
CLS = ch2id['<sos>']

# 加载字典: pinyin → [(word, weight), ...]
dict_path = os.path.join(OUT, 'dict.txt')
dict_map = {}
with open(dict_path, 'r', encoding='utf-8') as f:
    for line in f:
        parts = line.strip().split(',')
        if len(parts) >= 3:
            py, word = parts[0].strip(), parts[1].strip()
            try: w = int(parts[2].strip())
            except: w = 100
            dict_map.setdefault(py, []).append((word, w))

def split_pinyin(s):
    initials = ['zh','ch','sh','b','p','m','f','d','t','n','l','g','k','h',
                'j','q','x','r','z','c','s','y','w']
    finals = ['iang','iong','uang','ang','eng','ing','ong','uai','uan',
              'ian','iao','uan','uei','uen','ai','ei','ao','ou','an','en',
              'in','un','ia','ie','iu','uo','ua','ue','ui','er',
              'a','o','e','i','u','v']
    result = []
    i = 0
    while i < len(s):
        init = ''
        for ini in initials:
            if s[i:].startswith(ini):
                init = ini
                break
        if init:
            i += len(init)
            fin = ''
            for f in finals:
                if s[i:].startswith(f):
                    fin = f
                    break
            if fin:
                result.append(init + fin)
                i += len(fin)
            else:
                result.append(init)
        else:
            fin = ''
            for f in finals:
                if s[i:].startswith(f):
                    fin = f
                    break
            if fin:
                result.append(fin)
                i += len(fin)
            else:
                i += 1
    return result

def word_graph_search(syllables):
    """词图最短路径: 在音节序列上找所有可能的字典词, 拼成完整句子"""
    n = len(syllables)
    # best[i] = (最佳总权重, 回溯路径)
    # best[i] 表示从位置 i 到末尾的最佳分词
    INF = float('-inf')
    best = [None] * (n + 1)
    best[n] = (0, [])  # 到末尾, 分数=0, 空路径
    
    for i in range(n - 1, -1, -1):
        best_score = INF
        best_path = None
        # 尝试从位置 i 开始匹配长度为 1~min(6,n-i) 的词组
        for length in range(1, min(7, n - i + 1)):
            j = i + length
            if best[j] is None:
                continue
            # 拼接这几个音节
            py_key = ''.join(syllables[i:j])
            entries = dict_map.get(py_key, [])
            if not entries:
                continue
            # 取权重最高的词
            top_word, top_weight = max(entries, key=lambda x: x[1])
            # 偏好多字词: 给长词 bonus
            length_bonus = length * 200
            score = top_weight + length_bonus + best[j][0]
            if score > best_score:
                best_score = score
                best_path = [(top_word, py_key)] + best[j][1]
        
        # 单字fallback (如果字典里有单字)
        py1 = syllables[i]
        single_entries = dict_map.get(py1, [])
        if single_entries and best[i + 1] is not None:
            top_word, top_weight = max(single_entries, key=lambda x: x[1])
            score = top_weight + best[i + 1][0]
            if score > best_score:
                best_score = score
                best_path = [(top_word, py1)] + best[i + 1][1]
        
        if best_path is not None:
            best[i] = (best_score, best_path)
    
    if best[0] is None:
        return None
    
    return best[0][1]

print("=== 词图最短路径整句输入 ===\n")
tests = [
    ("suduhaishikeyide", "速度还是可以的"),
    ("wogujimingtianhuixiaxue", "我估计明天会下雪"),
    ("nihaoshijie", "你好世界"),
    ("gongzuoleyitian", "工作了一天"),
    ("jintiantianqibucuo", "今天天气不错"),
    ("ruguomingtianhaishixiaxue", "如果明天还是下雪"),
]

for pinyin, expected in tests:
    syllables = split_pinyin(pinyin)
    print(f"拼音: {pinyin}")
    print(f"音节: {' '.join(syllables)}")
    path = word_graph_search(syllables)
    if path:
        sentence = ''.join(w for w, _ in path)
        segments = ' | '.join(f'{w}({py})' for w, py in path)
        match = "✅" if sentence == expected else "❌"
        print(f"结果: {sentence}  {match}")
        print(f"分词: {segments}")
    else:
        print("结果: 无法分词 ❌")
    print(f"期望: {expected}\n")
