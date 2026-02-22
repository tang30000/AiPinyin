# 测试: AI对 "新的打字软件" vs "新的大子软件" 的评分
import json, numpy as np, onnxruntime as ort, os
os.environ['CUDA_VISIBLE_DEVICES']=''
OUT='target/debug'
sess = ort.InferenceSession(os.path.join(OUT,'weights.onnx'))
c2id = json.load(open(os.path.join(OUT,'char2id.json'),'r',encoding='utf-8'))
CLS = c2id['<sos>']

def score_sentence(ctx, sentence):
    ids = [CLS] + [c2id.get(c, 100) for c in ctx]
    total = 0.0
    for ch in sentence:
        inp = np.array([ids], dtype=np.int64)
        logits = sess.run(None, {'input_ids': inp})[0]
        ch_id = c2id.get(ch, 100)
        total += float(logits[0, -1, ch_id])
        ids.append(ch_id)
    return total

ctx = "这次使用了一个新的打字软件不知道这个"
s1 = score_sentence(ctx, "新的打字软件")
s2 = score_sentence(ctx, "新的大子软件")
s3 = score_sentence(ctx, "心底大子软件")
s4 = score_sentence(ctx, "新的鞑子软件")

print(f"新的打字软件: {s1:.1f}")
print(f"新的大子软件: {s2:.1f}")
print(f"心底大子软件: {s3:.1f}")
print(f"新的鞑子软件: {s4:.1f}")
