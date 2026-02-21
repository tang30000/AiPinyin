# -*- coding: utf-8 -*-
"""
从 Hugging Face Duyu/Pinyin2Hanzi-Transformer 加载模型
导出 ONNX + 词表 JSON

用法: python trainer/export_hf_model.py
"""
import sys
import os
import json
import numpy as np

import torch
import torch.nn as nn

# Force CPU for ONNX export: monkey-patch before importing run.py
torch.cuda.is_available = lambda: False

# 添加 hf_model 目录到路径
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'hf_model'))
from run import TransformerModel, PositionalEncoding, PinyinHanziTransformer

def main():
    pth_path = os.path.join(os.path.dirname(__file__), 'hf_model', 'pinyin2hanzi_transformer.pth')
    out_dir = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'target', 'debug')

    print(f"Loading model from {pth_path}...")
    wrapper = PinyinHanziTransformer.load(pth_path, device='cpu')
    model = wrapper.model
    dataset = wrapper.dataset
    model.eval()

    print(f"  Pinyin vocab: {len(dataset.pinyin_vocab)}")
    print(f"  Hanzi vocab:  {len(dataset.hanzi_vocab)}")
    print(f"  Max length:   {dataset.max_length}")
    print(f"  Config:       {wrapper.config}")

    # ── 测试推理 ──
    print("\nTest inference:")
    result = wrapper.predict("hong2 yan2 bo2 ming4", k=1, temperature=0.1)
    print(f"  'hong2 yan2 bo2 ming4' → '{result}'")
    result2 = wrapper.predict("ni3 hao3", k=1, temperature=0.1)
    print(f"  'ni3 hao3' → '{result2}'")

    # ── 导出词表 JSON ──
    print("\nExporting vocab...")

    # pinyin2id
    pinyin2id = {}
    for py, idx in dataset.pinyin2idx.items():
        pinyin2id[py] = idx
    with open(os.path.join(out_dir, 'pinyin2id.json'), 'w', encoding='utf-8') as f:
        json.dump(pinyin2id, f, ensure_ascii=False, indent=2)
    print(f"  pinyin2id.json: {len(pinyin2id)} entries")

    # char2id (hanzi)
    char2id = {}
    for ch, idx in dataset.hanzi2idx.items():
        char2id[ch] = idx
    with open(os.path.join(out_dir, 'char2id.json'), 'w', encoding='utf-8') as f:
        json.dump(char2id, f, ensure_ascii=False, indent=2)
    print(f"  char2id.json: {len(char2id)} entries")

    # vocab_meta
    meta = {
        'max_pinyin_len': dataset.max_length,
        'max_context_len': dataset.max_length,
        'model_type': 'Duyu/Pinyin2Hanzi-Transformer',
        'pinyin_vocab_size': len(dataset.pinyin_vocab),
        'hanzi_vocab_size': len(dataset.hanzi_vocab),
        'd_model': wrapper.config.get('d_model', 512),
        'encoder_decoder': True,
    }
    with open(os.path.join(out_dir, 'vocab_meta.json'), 'w', encoding='utf-8') as f:
        json.dump(meta, f, ensure_ascii=False, indent=2)
    print(f"  vocab_meta.json written")

    # ── 导出 ONNX ──
    print("\nExporting ONNX...")

    # 这个模型是 encoder-decoder，forward(pinyin, hanzi_input)
    # 对于 IME 我们需要逐字解码，所以导出完整模型
    # 输入: input_ids (pinyin), decoder_input_ids (hanzi 已生成部分)
    # 输出: logits

    max_len = dataset.max_length  # 14

    # 创建 wrapper 模型，去掉 device 依赖和 mask 的 .to(device)
    class ExportableModel(nn.Module):
        def __init__(self, orig_model):
            super().__init__()
            self.pinyin_embedding = orig_model.pinyin_embedding
            self.hanzi_embedding = orig_model.hanzi_embedding
            self.positional_encoding = orig_model.positional_encoding
            self.transformer = orig_model.transformer
            self.fc_out = orig_model.fc_out
            self.d_model = orig_model.d_model

        def forward(self, input_ids, decoder_input_ids):
            # Embedding
            src = self.pinyin_embedding(input_ids) * np.sqrt(self.d_model)
            tgt = self.hanzi_embedding(decoder_input_ids) * np.sqrt(self.d_model)

            # Positional encoding
            src = self.positional_encoding(src)
            tgt = self.positional_encoding(tgt)

            # Permute to (seq_len, batch, d_model)
            src = src.permute(1, 0, 2)
            tgt = tgt.permute(1, 0, 2)

            # Masks
            src_mask = torch.triu(torch.full((src.size(0), src.size(0)), float('-inf')), diagonal=1)
            tgt_mask = torch.triu(torch.full((tgt.size(0), tgt.size(0)), float('-inf')), diagonal=1)
            src_pad_mask = (input_ids == 0)
            tgt_pad_mask = (decoder_input_ids == 0)

            output = self.transformer(
                src=src, tgt=tgt,
                src_mask=src_mask, tgt_mask=tgt_mask,
                src_key_padding_mask=src_pad_mask,
                tgt_key_padding_mask=tgt_pad_mask,
                memory_key_padding_mask=src_pad_mask,
            )

            output = output.permute(1, 0, 2)
            logits = self.fc_out(output)
            return logits

    export_model = ExportableModel(model)
    export_model.eval()

    # 固定大小导出（IME 输入不超过 14 个音节）
    src_len = max_len
    tgt_len = max_len - 1

    dummy_input_ids = torch.zeros(1, src_len, dtype=torch.long)
    dummy_decoder_ids = torch.zeros(1, tgt_len, dtype=torch.long)
    dummy_input_ids[0, 0] = 2  # <sos>
    dummy_input_ids[0, 1] = 5
    dummy_input_ids[0, 2] = 3  # <eos>
    dummy_decoder_ids[0, 0] = 2  # <sos>

    onnx_path = os.path.join(out_dir, 'weights.onnx')

    # Use torch.jit.trace-based export (legacy mode, more compatible)
    with torch.no_grad():
        torch.onnx.export(
            export_model,
            (dummy_input_ids, dummy_decoder_ids),
            onnx_path,
            input_names=['input_ids', 'decoder_input_ids'],
            output_names=['logits'],
            opset_version=14,
            do_constant_folding=True,
            dynamo=False,  # Use legacy TorchScript-based export
        )

    onnx_size = os.path.getsize(onnx_path) / 1024 / 1024
    print(f"  ✅ weights.onnx: {onnx_size:.1f} MB → {onnx_path}")

    # ── 验证 ONNX ──
    try:
        import onnxruntime as ort
        sess = ort.InferenceSession(onnx_path)
        print(f"\n  ONNX inputs:  {[i.name for i in sess.get_inputs()]}")
        print(f"  ONNX outputs: {[o.name for o in sess.get_outputs()]}")

        # 测试推理
        result = sess.run(
            None,
            {
                'input_ids': dummy_input_ids.numpy(),
                'decoder_input_ids': dummy_decoder_ids.numpy(),
            }
        )
        print(f"  Output shape: {result[0].shape}")
        print("  ✅ ONNX 验证通过!")
    except ImportError:
        print("  ⚠ onnxruntime not installed, skipping validation")
    except Exception as e:
        print(f"  ⚠ ONNX validation error: {e}")

    print("\n✅ 全部完成!")
    print(f"   词表: {out_dir}/pinyin2id.json, char2id.json, vocab_meta.json")
    print(f"   模型: {out_dir}/weights.onnx")


if __name__ == '__main__':
    main()
