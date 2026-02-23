import onnxruntime as ort
import os
import sys

try:
    from onnxruntime.quantization import quantize_dynamic, QuantType
    fp32_model = "target/debug/_gpt2cn_temp/model.onnx"
    quantized_model = "target/debug/gpt2_int8.onnx"
    
    print(f"Quantizing {fp32_model} to INT8...")
    quantize_dynamic(
        model_input=fp32_model,
        model_output=quantized_model,
        weight_type=QuantType.QInt8,
    )
    
    # Also save a copy to project root
    import shutil
    shutil.copy2(quantized_model, "gpt2_int8.onnx")
    
    size_mb = os.path.getsize(quantized_model) / 1024 / 1024
    print(f"Success! Quantized model size: {size_mb:.1f} MB")
    
except Exception as e:
    print("Error:", e)
    sys.exit(1)
