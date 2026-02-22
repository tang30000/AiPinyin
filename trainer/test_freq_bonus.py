# -*- coding: utf-8 -*-
"""测试: AI分数 + 字频bonus 能否改善"""
import os, json, numpy as np, onnxruntime as ort
os.environ['CUDA_VISIBLE_DEVICES'] = ''
OUT = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'target', 'debug')
s = ort.InferenceSession(os.path.join(OUT, 'weights.onnx'))
py2id = json.load(open(os.path.join(OUT, 'pinyin2id.json'), 'r', encoding='utf-8'))
ch2id = json.load(open(os.path.join(OUT, 'char2id.json'), 'r', encoding='utf-8'))
p2c = json.load(open(os.path.join(OUT, 'pinyin2char.json'), 'r', encoding='utf-8'))
CLS = ch2id['<sos>']
c2py = {}
for py, chars in p2c.items():
    for c in chars:
        if c not in c2py: c2py[c] = py

# 加载字典统计字频
freq = {}
dict_path = os.path.join(OUT, 'dict.txt')
with open(dict_path, 'r', encoding='utf-8') as f:
    for line in f:
        parts = line.strip().split(',')
        if len(parts) >= 3:
            word = parts[1].strip()
            try:
                w = int(parts[2].strip())
            except ValueError:
                w = 100
            for c in word:
                freq[c] = freq.get(c, 0) + w
# 归一化
max_f = max(freq.values()) if freq else 1
char_freq = {c: v / max_f for c, v in freq.items()}

def run(ids):
    inp = np.array([ids], dtype=np.int64)
    return s.run(None, {
        'input_ids': inp,
        'attention_mask': np.ones_like(inp),
        'position_ids': np.arange(len(ids), dtype=np.int64).reshape(1, -1)
    })[0]

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

def generate(pinyin_str, freq_bonus=0.0):
    syllables = split_pinyin(pinyin_str)
    ids = [CLS]
    result = ''
    
    for syl in syllables:
        if syl not in py2id:
            continue
        ids.append(py2id[syl])
        logits = run(ids)
        last_logits = logits[0, -1, :]
        
        cands = p2c.get(syl, [])
        scores = []
        for c in cands:
            if c not in ch2id: continue
            ai_score = float(last_logits[ch2id[c]])
            bonus = char_freq.get(c, 0) * freq_bonus
            scores.append((c, ai_score + bonus, ai_score, bonus))
        scores.sort(key=lambda x: -x[1])
        
        if scores:
            best = scores[0][0]
            result += best
            ids.append(ch2id[best])
    
    return result

print("=== 字频bonus对比 ===\n")
tests = [
    ("suduhaishikeyide", "速度还是可以的"),
    ("wogujimingtianhuixiaxue", "我估计明天会下雪"),
    ("nihaoshijie", "你好世界"),
    ("gongzuoleyitian", "工作了一天"),
]

for pinyin, expected in tests:
    for bonus in [0, 1, 2, 3, 5]:
        r = generate(pinyin, bonus)
        match = "✅" if r == expected else "  "
        print(f"  bonus={bonus}: {r} {match}")
    print(f"  期望:    {expected}\n")
