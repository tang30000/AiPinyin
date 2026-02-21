# -*- coding: utf-8 -*-
"""快速验证 PinyinGPT 的排序能力"""
import os, json
os.environ['CUDA_VISIBLE_DEVICES'] = ''
import numpy as np
import onnxruntime as ort

OUT = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'target', 'debug')
sess = ort.InferenceSession(os.path.join(OUT, 'weights.onnx'))
py2id = json.load(open(os.path.join(OUT, 'pinyin2id.json'), 'r', encoding='utf-8'))
ch2id = json.load(open(os.path.join(OUT, 'char2id.json'), 'r', encoding='utf-8'))
id2ch = {v: k for k, v in ch2id.items()}

CLS = ch2id['<sos>']  # 101

def score_word(syllables, chars):
    """Concat 完整序列评分: [CLS] [py1] char1 [py2] char2 ..."""
    ids = [CLS]
    total = 0.0

    for syl, ch in zip(syllables, chars):
        py_id = py2id.get(syl)
        if py_id is None: return -999
        ids.append(py_id)

        inp = np.array([ids], dtype=np.int64)
        mask = np.ones_like(inp)
        pos = np.arange(len(ids), dtype=np.int64).reshape(1, -1)
        logits = sess.run(None, {'input_ids': inp, 'attention_mask': mask, 'position_ids': pos})[0]

        ch_id = ch2id.get(ch)
        if ch_id is None: return -999
        score = float(logits[0, -1, ch_id])
        total += score
        ids.append(ch_id)

    return total

# 测试用例
tests = [
    ("pibei",  ["pi", "bei"],  ["疲惫", "皮被", "被被", "劈背"]),
    ("wenti",  ["wen", "ti"],  ["问题", "文体", "文提", "闻啼"]),
    ("nihao",  ["ni", "hao"],  ["你好", "泥号", "尼耗"]),
    ("shi",    ["shi"],        ["是", "时", "事", "十", "使", "世"]),
    ("zhongguo", ["zhong", "guo"], ["中国", "终过", "重锅"]),
]

for pinyin, syls, cands in tests:
    print(f"\n=== {pinyin} ===")
    scores = []
    for word in cands:
        chars = list(word)
        if len(chars) != len(syls):
            scores.append((word, -999))
            continue
        s = score_word(syls, chars)
        scores.append((word, s))

    scores.sort(key=lambda x: -x[1])
    for i, (w, s) in enumerate(scores):
        marker = " ✅" if i == 0 else ""
        print(f"  {i+1}. {w} = {s:.2f}{marker}")
