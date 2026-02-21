# -*- coding: utf-8 -*-
"""验证 + 量化 PinyinGPT ONNX"""
import os, json
import numpy as np
import onnxruntime as ort

OUT_DIR = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'target', 'debug')

# 验证 FP32 模型
onnx_path = os.path.join(OUT_DIR, 'weights.onnx')
print(f"=== 验证 FP32 ONNX ({os.path.getsize(onnx_path)/1024/1024:.0f} MB) ===")
sess = ort.InferenceSession(onnx_path)
inputs = sess.get_inputs()
outputs = sess.get_outputs()
print(f"  Inputs:  {[(i.name, i.shape) for i in inputs]}")
print(f"  Outputs: {[(o.name, o.shape) for o in outputs]}")

dummy = np.zeros((1, 10), dtype=np.int64)
dummy[0, 0] = 101  # [CLS]
feed = {i.name: (np.ones_like(dummy) if 'mask' in i.name else dummy) for i in inputs}
result = sess.run(None, feed)
print(f"  Output shape: {result[0].shape}")
print(f"  ✅ FP32 验证OK")

# INT8 量化
print(f"\n=== INT8 量化 ===")
from onnxruntime.quantization import quantize_dynamic, QuantType

quant_path = os.path.join(OUT_DIR, 'weights_int8.onnx')
quantize_dynamic(
    onnx_path,
    quant_path,
    weight_type=QuantType.QInt8,
)

fp32_size = os.path.getsize(onnx_path) / 1024 / 1024
int8_size = os.path.getsize(quant_path) / 1024 / 1024
print(f"  FP32: {fp32_size:.0f} MB")
print(f"  INT8: {int8_size:.0f} MB ({int8_size/fp32_size*100:.0f}%)")
print(f"  压缩: {fp32_size/int8_size:.1f}x")

# 验证量化模型
sess2 = ort.InferenceSession(quant_path)
result2 = sess2.run(None, feed)
print(f"  Output shape: {result2[0].shape}")
print(f"  ✅ INT8 验证OK")

# 替换
backup = os.path.join(OUT_DIR, 'weights_fp32.onnx')
os.rename(onnx_path, backup)
os.rename(quant_path, onnx_path)
print(f"\n  ✅ weights.onnx = INT8 ({int8_size:.0f} MB)")
print(f"     weights_fp32.onnx = 备份 ({fp32_size:.0f} MB)")
