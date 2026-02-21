"""
AiPinyin ONNX 导出脚本

将训练好的 PyTorch 模型导出为 ONNX 格式,
兼容 ort 2.0 (opset 17)。

支持:
  - FP32 标准导出
  - FP16 半精度导出
  - INT8 动态量化 (推理加速, 模型体积减半)

用法:
  python trainer/export_onnx.py --checkpoint trainer/checkpoints/best_model.pt
  python trainer/export_onnx.py --checkpoint trainer/checkpoints/best_model.pt --quantize int8

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

    # FP16: 将模型权重转为 half
    if args.quantize == "fp16":
        model = model.half()
        print("[Export] 使用 FP16 半精度")

    # 构造示例输入
    max_py_len = 32
    max_ctx_len = 64
    dtype = torch.long
    dummy_pinyin = torch.zeros(1, max_py_len, dtype=dtype)
    dummy_context = torch.zeros(1, max_ctx_len, dtype=dtype)

    # 填入一些非零值
    dummy_pinyin[0, :3] = torch.tensor([4, 5, 6])
    dummy_context[0, :2] = torch.tensor([4, 5])

    # 导出 ONNX (先导出 FP32/FP16 版本)
    fp_output = args.output
    if args.quantize == "int8":
        # INT8 量化需要先导出 FP32, 再量化
        fp_output = args.output.replace(".onnx", "_fp32.onnx")

    print(f"[Export] 导出 ONNX → {fp_output}")

    # 使用 legacy TorchScript exporter (dynamo 会拆分外部数据文件)
    torch.onnx.export(
        model,
        (dummy_pinyin, dummy_context),
        fp_output,
        opset_version=17,  # ort 2.0 兼容
        input_names=["pinyin_ids", "context_ids"],
        output_names=["logits"],
        dynamic_axes={
            "pinyin_ids": {0: "batch", 1: "pinyin_len"},
            "context_ids": {0: "batch", 1: "context_len"},
            "logits": {0: "batch"},
        },
        do_constant_folding=True,
        dynamo=False,  # 强制使用 legacy exporter, 输出单文件
    )

    # 如果仍然生成了外部数据文件，合并回单文件
    ext_data = fp_output + ".data"
    if os.path.exists(ext_data):
        try:
            import onnx
            from onnx.external_data_helper import convert_model_to_external_data
            print("[Export] 合并外部数据到单文件...")
            onnx_model = onnx.load(fp_output, load_external_data=True)
            onnx.save_model(onnx_model, fp_output,
                           save_as_external_data=False)
            os.remove(ext_data)
        except Exception as e:
            print(f"[Export] ⚠ 合并外部数据失败: {e}")

    size_mb = os.path.getsize(fp_output) / 1024 / 1024
    print(f"[Export] ✅ 导出成功: {size_mb:.1f} MB")

    # INT8 动态量化
    final_output = fp_output
    if args.quantize == "int8":
        try:
            from onnxruntime.quantization import quantize_dynamic, QuantType
            print(f"[Export] INT8 动态量化中...")
            quantize_dynamic(
                fp_output,
                args.output,
                weight_type=QuantType.QInt8,
            )
            final_output = args.output
            q_size = os.path.getsize(args.output) / 1024 / 1024
            print(f"[Export] ✅ INT8 量化完成: {q_size:.1f} MB "
                  f"(压缩比: {size_mb/q_size:.1f}x)")
            # 清理 FP32 中间文件
            os.remove(fp_output)
        except ImportError:
            print("[Export] ⚠ 需要安装 onnxruntime: pip install onnxruntime")
            print("[Export] 使用 FP32 模型")
            final_output = fp_output
            # 重命名为目标文件名
            if fp_output != args.output:
                os.rename(fp_output, args.output)
                final_output = args.output
        except Exception as e:
            print(f"[Export] ⚠ INT8 量化失败: {e}")
            print("[Export] 使用 FP32 模型")
            if fp_output != args.output:
                os.rename(fp_output, args.output)
                final_output = args.output

    # 可选: ONNX 验证
    try:
        import onnx
        onnx_model = onnx.load(final_output)
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

        sess = ort.InferenceSession(final_output)
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
    out_dir = os.path.dirname(final_output) or "."
    for vocab_file in ["pinyin2id.json", "char2id.json", "vocab_meta.json"]:
        src = os.path.join(vocab_dir, vocab_file)
        dst = os.path.join(out_dir, vocab_file)
        if os.path.exists(src):
            shutil.copy2(src, dst)
            print(f"[Export] 复制 {vocab_file} → {out_dir}")

    print(f"\n[Export] 完成! 将以下文件放到 AiPinyin.exe 同目录:")
    print(f"  - {final_output}")
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
    parser.add_argument("--quantize", type=str, default="int8",
                        choices=["none", "fp16", "int8"],
                        help="量化方式: none=FP32, fp16=半精度, int8=INT8动态量化")
    args = parser.parse_args()
    export(args)
