# -*- coding: utf-8 -*-
"""
优化: 多字词优先 + 限单字展开 → 正确候选覆盖率
"""
import json, numpy as np, onnxruntime as ort, os, time
os.environ['CUDA_VISIBLE_DEVICES']=''

OUT='target/debug'
sess = ort.InferenceSession(os.path.join(OUT,'weights.onnx'))
c2id = json.load(open(os.path.join(OUT,'char2id.json'),'r',encoding='utf-8'))
CLS = c2id['<sos>']

dict_map = {}
with open(os.path.join(OUT, 'dict.txt'), 'r', encoding='utf-8') as f:
    for line in f:
        parts = line.strip().split(',', 2)
        if len(parts) >= 2:
            py, word = parts[0].strip(), parts[1].strip()
            dict_map.setdefault(py, []).append(word)

VALID = set(json.load(open(os.path.join(OUT, 'pinyin2char.json'),'r',encoding='utf-8')).keys())

def split_pinyin(s):
    result, i = [], 0
    while i < len(s):
        best = None
        for length in range(6, 0, -1):
            if s[i:i+length] in VALID:
                best = s[i:i+length]; break
        if best: result.append(best); i += len(best)
        else: i += 1
    return result

def ai_score(ctx, sentence):
    ids = [CLS] + [c2id.get(c, 100) for c in ctx]
    total = 0.0
    for ch in sentence:
        inp = np.array([ids], dtype=np.int64)
        logits = sess.run(None, {'input_ids': inp})[0]
        total += float(logits[0, -1, c2id.get(ch, 100)])
        ids.append(c2id.get(ch, 100))
    return total

def word_graph_smart(syllables, ctx='', top_k=5):
    """
    智能词图: 
    - 多字词(2+): 每个位置保留所有候选 (多字词数量少, 不爆炸)
    - 单字: 每个位置只保留 top-5 (按AI首字评分)
    """
    n = len(syllables)
    if n == 0: return []
    
    # 先用AI评估上下文, 获取每个位置的首选单字
    # (预先获取logits避免重复推理)
    
    best = [None] * (n + 1)
    best[n] = [[]]
    
    for i in range(n - 1, -1, -1):
        paths = []
        
        for length in range(1, min(7, n - i + 1)):
            j = i + length
            if best[j] is None: continue
            py_key = ''.join(syllables[i:j])
            words = dict_map.get(py_key, [])
            if not words: continue
            
            if length >= 2:
                # 多字词: 全部保留 (数量可控)
                selected = words
            else:
                # 单字: 只保留 top-10 (限制展开)
                selected = words[:10]
            
            for word in selected:
                for rest_path in best[j][:5]:  # 每个后续只取5条
                    paths.append([word] + rest_path)
        
        if paths:
            seen = set()
            uniq = []
            for p in paths:
                key = ''.join(p)
                if key not in seen:
                    seen.add(key)
                    uniq.append(p)
                    if len(uniq) >= 100:
                        break
            best[i] = uniq
    
    if best[0] is None: return []
    
    sentences = [''.join(p) for p in best[0]]
    print(f"  候选数: {len(sentences)}")
    
    scored = [(s, ai_score(ctx, s)) for s in sentences]
    scored.sort(key=lambda x: -x[1])
    return scored[:top_k]

tests = [
    ("xindedaziruanjian", "", "新的打字软件"),
    ("zhichidezenmyang", "", "支持的怎么样"),
    ("jintiantianqibucuo", "", "今天天气不错"),
    ("suduhaishikeyide", "", "速度还是可以的"),
    ("wogujimingtianhuixiaxue", "", "我估计明天会下雪"),
    ("keyidechugengduojielun", "", "可以得出更多结论"),
    ("gongzuoleyitian", "", "工作了一天"),
    ("ruguomingtianhaishixiaxue", "", "如果明天还是下雪"),
]

print("=== 多字词优先 + AI全评分 ===\n")
ok = 0
for pinyin, ctx, expected in tests:
    syllables = split_pinyin(pinyin)
    print(f"拼音: {pinyin} → {syllables}")
    t0 = time.time()
    results = word_graph_smart(syllables, ctx, 5)
    dt = time.time() - t0
    
    top = results[0][0] if results else '?'
    mark = "✅" if top == expected else "❌"
    if top == expected: ok += 1
    
    found_rank = -1
    for i, (s, sc) in enumerate(results):
        if s == expected: found_rank = i + 1
    
    for i, (s, score) in enumerate(results[:5]):
        flag = " ←" if s == expected else ""
        print(f"  #{i+1}: {s} (AI={score:.1f}){flag}")
    
    if found_rank > 0:
        print(f"  期望排名: #{found_rank}")
    else:
        print(f"  ⚠ '{expected}' 不在候选中!")
    print(f"  {mark} ({dt:.1f}s)\n")

print(f"准确率: {ok}/{len(tests)}")
