# -*- coding: utf-8 -*-
"""
手动导出 uer/gpt2-chinese-cluecorpussmall 为 ONNX
绕过 torch 2.10 的 FakeTensor bug: 用 torch.jit.trace + 手动导出
"""
import os, json, torch, numpy as np

OUT = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'target', 'debug')
MODEL_NAME = "uer/gpt2-chinese-cluecorpussmall"

# ── Step 1: 加载 ──
print("Step 1: 加载模型")
from transformers import GPT2LMHeadModel, BertTokenizer

tokenizer = BertTokenizer.from_pretrained(MODEL_NAME)
model = GPT2LMHeadModel.from_pretrained(MODEL_NAME)
model.eval()
vocab = tokenizer.get_vocab()
print(f"词表: {len(vocab)}, 参数: {sum(p.numel() for p in model.parameters())/1e6:.1f}M")

# ── Step 2: char2id ──
print("Step 2: 构建 char2id")
char2id = {}
for token, tid in vocab.items():
    if len(token) == 1:
        char2id[token] = tid
    elif token == '[CLS]':  char2id['<sos>'] = tid
    elif token == '[SEP]':  char2id['<eos>'] = tid
    elif token == '[UNK]':  char2id['<unk>'] = tid

with open(os.path.join(OUT, 'char2id.json'), 'w', encoding='utf-8') as f:
    json.dump(char2id, f, ensure_ascii=False, indent=0)
print(f"char2id: {len(char2id)} tokens")

# ── Step 3: 包装模型避免 FakeTensor 问题 ──
print("Step 3: 导出 ONNX")

class SimpleGPT2(torch.nn.Module):
    """简单包装, 只接受 input_ids, 返回 logits"""
    def __init__(self, gpt2):
        super().__init__()
        self.gpt2 = gpt2
    
    def forward(self, input_ids):
        outputs = self.gpt2(input_ids=input_ids)
        return outputs.logits

wrapper = SimpleGPT2(model)
wrapper.eval()

onnx_fp32 = os.path.join(OUT, 'weights_fp32.onnx')
onnx_int8 = os.path.join(OUT, 'weights.onnx')

dummy = torch.randint(0, len(vocab), (1, 16))

with torch.no_grad():
    # 设置环境变量跳过新版 dynamo 导出
    os.environ['TORCH_ONNX_USE_NEW_EXPORT'] = '0'
    torch.onnx.export(
        wrapper,
        dummy,
        onnx_fp32,
        input_names=['input_ids'],
        output_names=['logits'],
        dynamic_axes={
            'input_ids': {0: 'batch', 1: 'seq'},
            'logits': {0: 'batch', 1: 'seq'},
        },
        opset_version=14,
    )

fp32_mb = os.path.getsize(onnx_fp32) / (1024*1024)
print(f"FP32: {fp32_mb:.1f} MB")

# ── Step 4: 量化 ──
print("Step 4: INT8 量化")
from onnxruntime.quantization import quantize_dynamic, QuantType
quantize_dynamic(onnx_fp32, onnx_int8, weight_type=QuantType.QInt8)
int8_mb = os.path.getsize(onnx_int8) / (1024*1024)
print(f"INT8: {int8_mb:.1f} MB (压缩 {fp32_mb/int8_mb:.1f}x)")

# ── Step 5: 验证 ──
print("Step 5: 推理验证")
import onnxruntime as ort
os.environ['CUDA_VISIBLE_DEVICES'] = ''

sess = ort.InferenceSession(onnx_int8)
print(f"输入: {[i.name for i in sess.get_inputs()]}")
id2ch = {v: k for k, v in char2id.items()}

def predict_next(text):
    ids = [char2id.get('<sos>', 101)]
    for c in text:
        ids.append(char2id.get(c, char2id.get('<unk>', 100)))
    inp = np.array([ids], dtype=np.int64)
    logits = sess.run(None, {'input_ids': inp})[0]
    return logits[0, -1, :]

p2c = {}
p2c_path = os.path.join(OUT, 'pinyin2char.json')
if os.path.exists(p2c_path):
    p2c = json.load(open(p2c_path, 'r', encoding='utf-8'))

test_cases = [
    ("我今天", "qu", "去"),
    ("速度还是可", "yi", "以"),
    ("工", "zuo", "作"),
    ("估", "ji", "计"),
    ("今天天", "qi", "气"),
    ("可以得出更多结", "lun", "论"),
    ("如果明天还", "shi", "是"),
    ("这个速度还是可以", "de", "的"),
]

print("\n拼音约束测试:")
ok = 0
for ctx, py, expected in test_cases:
    logits = predict_next(ctx)
    cands = p2c.get(py, [])
    scores = [(c, float(logits[char2id[c]])) for c in cands if c in char2id]
    scores.sort(key=lambda x: -x[1])
    top = scores[0][0] if scores else '?'
    mark = "✅" if top == expected else "❌"
    if top == expected: ok += 1
    top5 = ', '.join(f'{c}={s:.1f}' for c, s in scores[:5])
    print(f"  '{ctx}'+[{py}] → {top} {mark}  ({top5})")

print(f"\n准确率: {ok}/{len(test_cases)}")

os.remove(onnx_fp32)
print(f"\n✅ 完成! {onnx_int8} ({int8_mb:.1f}MB)")
