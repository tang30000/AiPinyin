# -*- coding: utf-8 -*-
"""
PinyinGPT (aihijo/transformers4ime-pinyingpt-concat)
PyTorch → ONNX 导出 + INT8 量化 + 词表导出

用法: python trainer/export_pinyingpt.py
"""
import os, sys, json, shutil
os.environ['CUDA_VISIBLE_DEVICES'] = ''  # Force CPU

import torch
import numpy as np
from transformers import BertTokenizer, GPT2LMHeadModel, GPT2Config

MODEL_DIR = os.path.join(os.path.dirname(__file__), 'hf_model', 'pinyingpt')
OUT_DIR = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'target', 'debug')


def load_model():
    """加载模型 (修正 vocab_size)"""
    print("=== 1. 加载模型 ===")

    # 修正 config: vocab_size 需要包含扩展的拼音 tokens
    with open(os.path.join(MODEL_DIR, 'config.json')) as f:
        config_dict = json.load(f)

    # 加载词表获取真正大小
    with open(os.path.join(MODEL_DIR, 'additional_special_tokens.json'), encoding='utf-8') as f:
        extra_tokens = json.load(f)

    real_vocab_size = config_dict['vocab_size'] + len(extra_tokens)
    print(f"  原始 vocab_size: {config_dict['vocab_size']}")
    print(f"  扩展拼音 tokens: {len(extra_tokens)}")
    print(f"  真正 vocab_size: {real_vocab_size}")

    config_dict['vocab_size'] = real_vocab_size
    config_dict['attn_implementation'] = 'eager'  # Avoid SDPA for ONNX export
    config = GPT2Config(**config_dict)

    # 加载模型
    model = GPT2LMHeadModel(config)
    state = torch.load(os.path.join(MODEL_DIR, 'pytorch_model.bin'),
                       map_location='cpu', weights_only=False)
    missing, unexpected = model.load_state_dict(state, strict=False)
    if missing:
        print(f"  ⚠ Missing keys: {len(missing)} (expected for buffers)")
    if unexpected:
        print(f"  ⚠ Unexpected keys: {len(unexpected)}")
    model.eval()

    param_count = sum(p.numel() for p in model.parameters()) / 1e6
    print(f"  模型参数: {param_count:.1f}M")
    print(f"  结构: n_layer={config.n_layer}, n_head={config.n_head}, n_embd={config.n_embd}")

    # 加载 tokenizer
    tokenizer = BertTokenizer.from_pretrained(MODEL_DIR,
        additional_special_tokens=extra_tokens)
    print(f"  Tokenizer 词表: {tokenizer.vocab_size + len(extra_tokens)}")

    return model, tokenizer, config, extra_tokens


def test_inference(model, tokenizer, extra_tokens):
    """测试推理"""
    print("\n=== 2. 推理测试 ===")

    # PinyinGPT concat 格式: 拼音token和汉字token交替拼接
    # 输入: [CLS] [ni] 你 [hao] 好 [SEP]
    # 或: 上文汉字 + 拼音tokens, 模型预测对应汉字
    test_cases = [
        # 格式: 拼音token列表 (用 [xx] 形式的特殊 token)
        ["[ni]", "[hao]"],
        ["[wo]", "[shi]", "[zhong]", "[guo]", "[ren]"],
        ["[jin]", "[tian]", "[tian]", "[qi]"],
    ]

    # pinyin2char 映射
    with open(os.path.join(MODEL_DIR, 'pinyin2char.json'), encoding='utf-8') as f:
        pinyin2char = json.load(f)

    for pinyins in test_cases:
        pinyin_str = ' '.join(p.strip('[]') for p in pinyins)

        # 构建输入: [CLS] + 拼音tokens
        input_ids = [tokenizer.cls_token_id]
        for py in pinyins:
            py_id = tokenizer.convert_tokens_to_ids(py)
            input_ids.append(py_id)

        input_tensor = torch.tensor([input_ids], dtype=torch.long)

        # 贪心解码 (逐步: 拼音token → 预测汉字 → 下一个拼音token → ...)
        # Concat 格式: 输入序列是 [CLS] [py1] char1 [py2] char2 ...
        result_chars = []
        current_ids = [tokenizer.cls_token_id]

        for py_token in pinyins:
            py_id = tokenizer.convert_tokens_to_ids(py_token)
            current_ids.append(py_id)

            input_tensor = torch.tensor([current_ids], dtype=torch.long)
            with torch.no_grad():
                outputs = model(input_tensor)
                logits = outputs.logits[0, -1, :]  # 最后一个位置的 logits

            # 约束: 只在该拼音对应的候选汉字中选择
            py_key = py_token.strip('[]')
            if py_key in pinyin2char:
                candidates = pinyin2char[py_key]
                candidate_ids = [tokenizer.convert_tokens_to_ids(c) for c in candidates]
                candidate_ids = [cid for cid in candidate_ids if cid != tokenizer.unk_token_id]

                if candidate_ids:
                    mask = torch.full_like(logits, float('-inf'))
                    for cid in candidate_ids:
                        mask[cid] = logits[cid]
                    best_id = mask.argmax().item()
                else:
                    best_id = logits.argmax().item()
            else:
                best_id = logits.argmax().item()

            best_char = tokenizer.convert_ids_to_tokens(best_id)
            result_chars.append(best_char)
            current_ids.append(best_id)

        result = ''.join(result_chars)
        print(f"  '{pinyin_str}' → '{result}'")


def export_vocab(tokenizer, extra_tokens):
    """导出词表 JSON"""
    print("\n=== 3. 导出词表 ===")

    # pinyin2id: 无声调拼音 → token ID
    pinyin2id = {}
    for token in extra_tokens:
        py = token.strip('[]')
        tid = tokenizer.convert_tokens_to_ids(token)
        if tid != tokenizer.unk_token_id:
            pinyin2id[py] = tid

    with open(os.path.join(OUT_DIR, 'pinyin2id.json'), 'w', encoding='utf-8') as f:
        json.dump(pinyin2id, f, ensure_ascii=False, indent=2)
    print(f"  pinyin2id.json: {len(pinyin2id)} 条")

    # char2id: 汉字 → token ID
    vocab = tokenizer.get_vocab()
    char2id = {}
    for token, tid in vocab.items():
        if len(token) == 1 and '\u4e00' <= token <= '\u9fff':
            char2id[token] = tid
        elif token in ['，', '。', '！', '？', '、', '；', '：']:
            char2id[token] = tid
    # 也加上特殊 tokens
    char2id['<pad>'] = tokenizer.pad_token_id if tokenizer.pad_token_id is not None else 0
    char2id['<unk>'] = tokenizer.unk_token_id
    char2id['<sos>'] = tokenizer.cls_token_id
    char2id['<eos>'] = tokenizer.sep_token_id

    with open(os.path.join(OUT_DIR, 'char2id.json'), 'w', encoding='utf-8') as f:
        json.dump(char2id, f, ensure_ascii=False, indent=2)
    print(f"  char2id.json: {len(char2id)} 条")

    # pinyin2char 映射 (直接复制)
    src = os.path.join(MODEL_DIR, 'pinyin2char.json')
    dst = os.path.join(OUT_DIR, 'pinyin2char.json')
    shutil.copy2(src, dst)
    with open(src, encoding='utf-8') as f:
        p2c = json.load(f)
    print(f"  pinyin2char.json: {len(p2c)} 条 (复制)")

    # vocab_meta
    meta = {
        'model_type': 'PinyinGPT-Concat',
        'model_id': 'aihijo/transformers4ime-pinyingpt-concat',
        'architecture': 'GPT2LMHeadModel',
        'vocab_size': len(vocab) + len(extra_tokens),
        'n_embd': 768,
        'n_layer': 12,
        'n_head': 12,
        'max_pinyin_len': 14,
        'max_context_len': 1024,
        'pinyin_format': 'toneless_bracketed',  # [ni] [hao]
        'encoder_decoder': False,
        'concat_mode': True,  # 拼音汉字交替拼接
    }
    with open(os.path.join(OUT_DIR, 'vocab_meta.json'), 'w', encoding='utf-8') as f:
        json.dump(meta, f, ensure_ascii=False, indent=2)
    print(f"  vocab_meta.json written")


def export_onnx(model, tokenizer):
    """导出 ONNX (无 KV Cache, 适合短句)"""
    print("\n=== 4. 导出 ONNX ===")

    onnx_path = os.path.join(OUT_DIR, 'weights.onnx')

    # 方法: 保存为 HF 格式后用 optimum 导出
    import tempfile
    with tempfile.TemporaryDirectory() as tmpdir:
        # 保存修正后的模型 + tokenizer
        model.config.vocab_size = 21571
        model.save_pretrained(tmpdir)
        tokenizer.save_pretrained(tmpdir)

        # 用 optimum CLI 导出
        import subprocess
        cmd = [
            sys.executable, '-m', 'optimum.exporters.onnx',
            '--model', tmpdir,
            '--task', 'text-generation',
            '--no-post-process',
            OUT_DIR,
        ]
        print(f"  Running: {' '.join(cmd[-4:])}")
        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode != 0:
            print(f"  stderr: {result.stderr[-500:]}")
            raise RuntimeError("optimum export failed")

    # optimum 输出的文件名可能是 model.onnx 不是 weights.onnx
    optimum_onnx = os.path.join(OUT_DIR, 'model.onnx')
    if os.path.exists(optimum_onnx) and not os.path.exists(onnx_path):
        os.rename(optimum_onnx, onnx_path)
    elif os.path.exists(os.path.join(OUT_DIR, 'decoder_model.onnx')):
        # optimum 可能输出 decoder_model.onnx
        os.rename(os.path.join(OUT_DIR, 'decoder_model.onnx'), onnx_path)

    onnx_size = os.path.getsize(onnx_path) / 1024 / 1024
    print(f"  ✅ weights.onnx: {onnx_size:.1f} MB")

    # 验证
    import onnxruntime as ort
    sess = ort.InferenceSession(onnx_path)
    inputs = sess.get_inputs()
    outputs = sess.get_outputs()
    print(f"  ONNX inputs: {[i.name for i in inputs]}")
    print(f"  ONNX outputs: {[o.name for o in outputs]}")

    # 测试
    dummy = np.zeros((1, 10), dtype=np.int64)
    dummy[0, 0] = 101  # [CLS]
    feed = {inputs[0].name: dummy}
    # 如果有 attention_mask 输入
    if len(inputs) > 1 and 'attention_mask' in inputs[1].name:
        feed[inputs[1].name] = np.ones((1, 10), dtype=np.int64)
    result = sess.run(None, feed)
    print(f"  Output shape: {result[0].shape}")
    print(f"  ✅ ONNX 验证通过!")

    return onnx_path


def quantize_int8(onnx_path):
    """INT8 动态量化"""
    print("\n=== 5. INT8 量化 ===")

    from onnxruntime.quantization import quantize_dynamic, QuantType

    quantized_path = onnx_path.replace('.onnx', '_int8.onnx')
    quantize_dynamic(
        onnx_path,
        quantized_path,
        weight_type=QuantType.QInt8,
        optimize_model=True,
    )

    orig_size = os.path.getsize(onnx_path) / 1024 / 1024
    quant_size = os.path.getsize(quantized_path) / 1024 / 1024
    ratio = quant_size / orig_size * 100

    print(f"  原始: {orig_size:.1f} MB")
    print(f"  量化: {quant_size:.1f} MB ({ratio:.0f}%)")
    print(f"  压缩比: {orig_size/quant_size:.1f}x")

    # 验证量化模型
    import onnxruntime as ort
    sess = ort.InferenceSession(quantized_path)
    dummy = np.zeros((1, 10), dtype=np.int64)
    dummy[0, 0] = 101  # [CLS]
    result = sess.run(None, {'input_ids': dummy})
    print(f"  ✅ 量化模型验证通过! Output shape: {result[0].shape}")

    # 用量化版替换
    final_path = os.path.join(OUT_DIR, 'weights.onnx')
    backup_path = os.path.join(OUT_DIR, 'weights_fp32.onnx')
    os.rename(final_path, backup_path)
    os.rename(quantized_path, final_path)

    quant_final_size = os.path.getsize(final_path) / 1024 / 1024
    print(f"  ✅ 量化模型已部署: weights.onnx = {quant_final_size:.1f} MB")
    print(f"     FP32 备份: weights_fp32.onnx")

    return final_path


def main():
    model, tokenizer, config, extra_tokens = load_model()
    test_inference(model, tokenizer, extra_tokens)
    export_vocab(tokenizer, extra_tokens)
    onnx_path = export_onnx(model, tokenizer)
    quantize_int8(onnx_path)

    print("\n" + "=" * 50)
    print("✅ 全部完成!")
    print(f"   模型: {OUT_DIR}/weights.onnx (INT8 量化)")
    print(f"   词表: pinyin2id.json, char2id.json, pinyin2char.json")
    print(f"   元数据: vocab_meta.json")
    print("=" * 50)


if __name__ == '__main__':
    main()
