# -*- coding: utf-8 -*-
"""验证上下文对 PinyinGPT 评分的影响"""
import os, json
os.environ['CUDA_VISIBLE_DEVICES'] = ''
import numpy as np
import onnxruntime as ort

OUT = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'target', 'debug')
sess = ort.InferenceSession(os.path.join(OUT, 'weights.onnx'))
py2id = json.load(open(os.path.join(OUT, 'pinyin2id.json'), 'r', encoding='utf-8'))
ch2id = json.load(open(os.path.join(OUT, 'char2id.json'), 'r', encoding='utf-8'))
CLS = ch2id['<sos>']

def run(ids):
    inp = np.array([ids], dtype=np.int64)
    mask = np.ones_like(inp)
    pos = np.arange(len(ids), dtype=np.int64).reshape(1, -1)
    return sess.run(None, {'input_ids': inp, 'attention_mask': mask, 'position_ids': pos})[0]

def score_with_context(context_chars, syl, candidates):
    """带上下文评分: [CLS] ctx1 ctx2 ... [py] → 各候选字分数"""
    ids = [CLS]
    for ch in context_chars:
        cid = ch2id.get(ch)
        if cid: ids.append(cid)

    py_id = py2id.get(syl)
    if not py_id: return {}
    ids.append(py_id)

    logits = run(ids)
    results = {}
    for word in candidates:
        ch = word[0]  # 首字
        cid = ch2id.get(ch)
        if cid:
            results[word] = float(logits[0, -1, cid])
    return results

# 测试: 不同上下文下的排序
tests = [
    {
        'pinyin': 'wenti',
        'syl': 'wen',
        'candidates': ['问题', '文体', '文提', '闻啼'],
        'contexts': [
            [],                         # 无上下文
            list("这个"),               # 短上下文
            list("我想问你一个"),        # 有"问"的上下文
            list("这篇文章有什么"),       # 有"文"的上下文
        ]
    },
    {
        'pinyin': 'pibei',
        'syl': 'pi',
        'candidates': ['疲惫', '皮被', '劈背'],
        'contexts': [
            [],
            list("我今天很"),
            list("工作了一天感觉很"),
        ]
    },
    {
        'pinyin': 'shi',
        'syl': 'shi',
        'candidates': ['是', '时', '事', '十'],
        'contexts': [
            [],
            list("我"),
            list("今天下午两点的"),
        ]
    },
]

for test in tests:
    print(f"\n{'='*50}")
    print(f"拼音: {test['pinyin']}")
    for ctx in test['contexts']:
        ctx_str = ''.join(ctx) if ctx else '(无)'
        scores = score_with_context(ctx, test['syl'], test['candidates'])
        ranked = sorted(scores.items(), key=lambda x: -x[1])
        ranking = ', '.join(f"{w}={s:.1f}" for w, s in ranked)
        print(f"  上下文 [{ctx_str}] → {ranking}")
