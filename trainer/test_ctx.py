# -*- coding: utf-8 -*-
import json, numpy as np, onnxruntime as ort, os
os.environ['CUDA_VISIBLE_DEVICES']=''
OUT='target/debug'
sess = ort.InferenceSession(os.path.join(OUT,'weights.onnx'))
c2id = json.load(open(os.path.join(OUT,'char2id.json'),'r',encoding='utf-8'))
p2c = json.load(open(os.path.join(OUT,'pinyin2char.json'),'r',encoding='utf-8'))
CLS = c2id['<sos>']

def predict_next(text):
    ids = [CLS] + [c2id.get(c, c2id.get('<unk>',100)) for c in text]
    inp = np.array([ids], dtype=np.int64)
    logits = sess.run(None, {'input_ids': inp})[0]
    return logits[0, -1, :]

# 无上下文
logits0 = predict_next('')
cands = p2c.get('zhi',[])
scores0 = [(c, float(logits0[c2id[c]])) for c in cands if c in c2id]
scores0.sort(key=lambda x:-x[1])
print('无上下文 zhi:', ', '.join(f'{c}={s:.1f}' for c,s in scores0[:5]))

# 有上下文
ctx = '打字软件用的是全拼打字法不知道对首字母打字'
logits1 = predict_next(ctx)
scores1 = [(c, float(logits1[c2id[c]])) for c in cands if c in c2id]
scores1.sort(key=lambda x:-x[1])
print('有上下文 zhi:', ', '.join(f'{c}={s:.1f}' for c,s in scores1[:5]))

zhi = c2id.get('支', 0)
zhi2 = c2id.get('之', 0)
zhi3 = c2id.get('知', 0)
print(f'\n  支: 无ctx={logits0[zhi]:.1f}  有ctx={logits1[zhi]:.1f}')
print(f'  之: 无ctx={logits0[zhi2]:.1f}  有ctx={logits1[zhi2]:.1f}')
print(f'  知: 无ctx={logits0[zhi3]:.1f}  有ctx={logits1[zhi3]:.1f}')
