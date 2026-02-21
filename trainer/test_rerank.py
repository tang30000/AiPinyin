# -*- coding: utf-8 -*-
"""对比 Concat 正确格式 vs 纯文字格式"""
import os,json,numpy as np,onnxruntime as ort
os.environ['CUDA_VISIBLE_DEVICES']=''
OUT=os.path.join(os.path.dirname(os.path.dirname(__file__)),'target','debug')
s=ort.InferenceSession(os.path.join(OUT,'weights.onnx'))
py2id=json.load(open(os.path.join(OUT,'pinyin2id.json'),'r',encoding='utf-8'))
ch2id=json.load(open(os.path.join(OUT,'char2id.json'),'r',encoding='utf-8'))
p2c=json.load(open(os.path.join(OUT,'pinyin2char.json'),'r',encoding='utf-8'))
CLS=ch2id['<sos>']
c2py={}
for py,chars in p2c.items():
    for c in chars:
        if c not in c2py: c2py[c]=py

def run(ids):
    inp=np.array([ids],dtype=np.int64)
    return s.run(None,{'input_ids':inp,'attention_mask':np.ones_like(inp),
        'position_ids':np.arange(len(ids),dtype=np.int64).reshape(1,-1)})[0]

def test(ctx, target_py, target_char, mode='concat'):
    ids=[CLS]
    if mode=='concat':
        for c in ctx:
            py=c2py.get(c)
            if py and py in py2id and c in ch2id:
                ids.append(py2id[py])
                ids.append(ch2id[c])
    else:  # raw
        for c in ctx:
            if c in ch2id: ids.append(ch2id[c])
    ids.append(py2id[target_py])
    L=run(ids)[0,-1,:]
    cands=[(c,float(L[ch2id[c]])) for c in p2c.get(target_py,[]) if c in ch2id]
    cands.sort(key=lambda x:-x[1])
    rank=[i for i,(c,_) in enumerate(cands) if c==target_char]
    r=rank[0]+1 if rank else -1
    top3=', '.join(f'{c}={s:.1f}' for c,s in cands[:5])
    print(f'  [{mode:6s}] ctx="{ctx}" → {target_char} 排{r:>2}/{len(cands)}  top5: {top3}')

cases = [
    ('我', 'gu', '估'),
    ('我估', 'ji', '计'),
    ('今天下雨我', 'gu', '估'),
    ('今天下雨我估', 'ji', '计'),
    ('我想问你一个', 'wen', '问'),
    ('工作了一天感觉', 'pi', '疲'),
    ('我', 'shi', '是'),
    ('你', 'hao', '好'),
    ('', 'gu', '估'),
    ('', 'wen', '问'),
]

print('=== Concat格式 vs 纯文字格式 对比 ===\n')
for ctx, py, ch in cases:
    test(ctx, py, ch, 'concat')
    test(ctx, py, ch, 'raw')
    print()
