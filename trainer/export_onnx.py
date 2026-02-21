"""
AiPinyin ONNX 导出脚本

将训练好的 PyTorch 模型导出为 ONNX 格式,
兼容 ort 2.0 (opset 17)。

用法:
  python trainer/export_onnx.py --checkpoint trainer/checkpoints/best_model.pt

输出:
  weights.onnx — 直接放到 AiPinyin.exe 旁边即可使用
"""

import os
import sys
import json
import argparse
import shutil

import torch

sys.path.insert(0, os.path.dirname(__file__))
from model import PinyinTransformer
from tokenizer import PinyinTokenizer, CharTokenizer


def export(args):
    device = torch.device("cpu")  # 导出用 CPU

    # 加载词表信息
    vocab_dir = os.path.join(os.path.dirname(__file__), "vocab")
    meta_path = os.path.join(vocab_dir, "vocab_meta.json")

    if os.path.exists(meta_path):
        with open(meta_path, "r") as f:
            meta = json.load(f)
        pinyin_vocab = meta["pinyin_vocab_size"]
        char_vocab = meta["char_vocab_size"]
    else:
        py_tok = PinyinTokenizer()
        ch_tok = CharTokenizer()
        pinyin_vocab = py_tok.vocab_size
        char_vocab = ch_tok.vocab_size

    print(f"[Export] 拼音词表: {pinyin_vocab}, 汉字词表: {char_vocab}")

    # 创建模型
    model = PinyinTransformer(
        pinyin_vocab_size=pinyin_vocab,
        char_vocab_size=char_vocab,
    )

    # 加载权重
    if os.path.exists(args.checkpoint):
        ckpt = torch.load(args.checkpoint, map_location=device, weights_only=False)
        if "model_state_dict" in ckpt:
            model.load_state_dict(ckpt["model_state_dict"])
            print(f"[Export] 加载 checkpoint: epoch={ckpt.get('epoch', '?')}, "
                  f"loss={ckpt.get('loss', '?')}")
        else:
            model.load_state_dict(ckpt)
            print(f"[Export] 加载模型权重")
    else:
        print(f"[Export] ⚠ 未找到 checkpoint: {args.checkpoint}")
        print(f"[Export] 导出随机初始化模型（仅供测试）")

    model.eval()

    # 构造示例输入
    max_py_len = 32
    max_ctx_len = 64
    dummy_pinyin = torch.zeros(1, max_py_len, dtype=torch.long)
    dummy_context = torch.zeros(1, max_ctx_len, dtype=torch.long)

    # 填入一些非零值
    dummy_pinyin[0, :3] = torch.tensor([4, 5, 6])  # 一些拼音 ID
    dummy_context[0, :2] = torch.tensor([4, 5])     # 一些汉字 ID

    # 导出 ONNX
    output_path = args.output
    print(f"[Export] 导出 ONNX → {output_path}")

    torch.onnx.export(
        model,
        (dummy_pinyin, dummy_context),
        output_path,
        opset_version=17,  # ort 2.0 兼容
        input_names=["pinyin_ids", "context_ids"],
        output_names=["logits"],
        dynamic_axes={
            "pinyin_ids": {0: "batch", 1: "pinyin_len"},
            "context_ids": {0: "batch", 1: "context_len"},
            "logits": {0: "batch"},
        },
        do_constant_folding=True,
    )

    # 验证
    size_mb = os.path.getsize(output_path) / 1024 / 1024
    print(f"[Export] ✅ 导出成功: {size_mb:.1f} MB")

    # 可选: ONNX 验证
    try:
        import onnx
        onnx_model = onnx.load(output_path)
        onnx.checker.check_model(onnx_model)
        print(f"[Export] ✅ ONNX 模型验证通过")
    except ImportError:
        print(f"[Export] ℹ 安装 onnx 包可进行模型验证: pip install onnx")
    except Exception as e:
        print(f"[Export] ⚠ ONNX 验证错误: {e}")

    # 可选: ONNX Runtime 验证
    try:
        import onnxruntime as ort
        import numpy as np

        sess = ort.InferenceSession(output_path)
        result = sess.run(
            None,
            {
                "pinyin_ids": dummy_pinyin.numpy(),
                "context_ids": dummy_context.numpy(),
            },
        )
        print(f"[Export] ✅ ORT 推理验证通过, 输出 shape: {result[0].shape}")
    except ImportError:
        print(f"[Export] ℹ 安装 onnxruntime 可进行推理验证: pip install onnxruntime")
    except Exception as e:
        print(f"[Export] ⚠ ORT 验证错误: {e}")

    # 复制词表文件到输出目录
    out_dir = os.path.dirname(output_path) or "."
    for vocab_file in ["pinyin2id.json", "char2id.json", "vocab_meta.json"]:
        src = os.path.join(vocab_dir, vocab_file)
        dst = os.path.join(out_dir, vocab_file)
        if os.path.exists(src):
            shutil.copy2(src, dst)
            print(f"[Export] 复制 {vocab_file} → {out_dir}")

    print(f"\n[Export] 完成! 将以下文件放到 AiPinyin.exe 同目录:")
    print(f"  - {output_path}")
    print(f"  - pinyin2id.json")
    print(f"  - char2id.json")
    print(f"  - vocab_meta.json")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="导出 ONNX 模型")
    parser.add_argument("--checkpoint", type=str,
                        default="trainer/checkpoints/best_model.pt",
                        help="PyTorch checkpoint 路径")
    parser.add_argument("--output", type=str,
                        default="weights.onnx",
                        help="ONNX 输出路径")
    args = parser.parse_args()
    export(args)
