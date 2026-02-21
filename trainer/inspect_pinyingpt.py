# -*- coding: utf-8 -*-
"""
PinyinGPT 探查 v2 - 直接加载权重检查 shape
"""
import os, json
os.environ['CUDA_VISIBLE_DEVICES'] = ''
import torch

print("=== 下载权重文件 ===")
from huggingface_hub import hf_hub_download

cache_dir = os.path.join(os.path.dirname(__file__), 'hf_model', 'pinyingpt')
os.makedirs(cache_dir, exist_ok=True)

files_to_download = [
    'config.json', 'vocab.txt', 'special_tokens_map.json',
    'tokenizer_config.json', 'pinyin2char.json',
    'additional_special_tokens.json', 'pytorch_model.bin',
]

for f in files_to_download:
    print(f"  下载 {f}...")
    hf_hub_download(
        "aihijo/transformers4ime-pinyingpt-concat", f,
        local_dir=cache_dir
    )

print("\n=== 检查权重 shape ===")
state = torch.load(os.path.join(cache_dir, 'pytorch_model.bin'), map_location='cpu')
for k, v in state.items():
    if 'wte' in k or 'wpe' in k or 'lm_head' in k or 'ln_f' in k:
        print(f"  {k}: {v.shape}")

# 看 config
with open(os.path.join(cache_dir, 'config.json')) as f:
    config = json.load(f)
print(f"\n=== Config ===")
print(f"  vocab_size: {config['vocab_size']}")
print(f"  n_embd: {config['n_embd']}")
print(f"  n_layer: {config['n_layer']}")

# 看 vocab 大小
with open(os.path.join(cache_dir, 'vocab.txt'), encoding='utf-8') as f:
    vocab_lines = f.readlines()
print(f"\n=== Vocab ===")
print(f"  vocab.txt 行数: {len(vocab_lines)}")
print(f"  前20行: {[l.strip() for l in vocab_lines[:20]]}")

# 看 additional_special_tokens
with open(os.path.join(cache_dir, 'additional_special_tokens.json'), encoding='utf-8') as f:
    extra_tokens = json.load(f)
print(f"\n=== 额外特殊 tokens: {len(extra_tokens)} 个 ===")
print(f"  前30: {extra_tokens[:30]}")

# 看 pinyin2char
with open(os.path.join(cache_dir, 'pinyin2char.json'), encoding='utf-8') as f:
    p2c = json.load(f)
print(f"\n=== pinyin2char ===")
print(f"  条目数: {len(p2c)}")
sample = list(p2c.items())[:5]
for k, v in sample:
    if isinstance(v, list):
        print(f"  '{k}' → {v[:5]}...")
    else:
        print(f"  '{k}' → {v}")

# 真正的 embedding 大小
wte_shape = state['transformer.wte.weight'].shape
print(f"\n=== 实际 Embedding ===")
print(f"  wte: {wte_shape} → 真正 vocab_size = {wte_shape[0]}")
print(f"  config 声称: {config['vocab_size']}")
if wte_shape[0] != config['vocab_size']:
    print(f"  ⚠ 不匹配! 需要修改 config.json 的 vocab_size 为 {wte_shape[0]}")
