import sys
print("Python:", sys.version)
try:
    import torch
    print("PyTorch:", torch.__version__)
    print("CUDA:", torch.cuda.is_available())
    if torch.cuda.is_available():
        print("GPU:", torch.cuda.get_device_name(0))
        mem = torch.cuda.get_device_properties(0).total_memory
        print("VRAM: {:.1f} GB".format(mem / 1e9))
except ImportError:
    print("PyTorch: NOT INSTALLED")

try:
    import onnx
    print("ONNX:", onnx.__version__)
except ImportError:
    print("ONNX: NOT INSTALLED")

try:
    import onnxruntime
    print("ONNX Runtime:", onnxruntime.__version__)
except ImportError:
    print("ONNX Runtime: NOT INSTALLED")
