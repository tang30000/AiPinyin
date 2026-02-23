import onnxruntime as ort
import numpy as np
import sys

try:
    print("Loading gpt2_int8.onnx...")
    sess = ort.InferenceSession("target/debug/gpt2_int8.onnx", providers=['CPUExecutionProvider'])
    
    print("\n--- Inputs ---")
    inputs = sess.get_inputs()
    for i in inputs:
        print(f"Name: {i.name}, Shape: {i.shape}, Type: {i.type}")
        
    print("\n--- Outputs ---")
    for o in sess.get_outputs():
        print(f"Name: {o.name}, Shape: {o.shape}, Type: {o.type}")

    print("\n--- Testing forward pass ---")
    dummy_input = np.array([[101, 102]], dtype=np.int64)
    feed = {inputs[0].name: dummy_input}
    result = sess.run(None, feed)
    print(f"Success! Output shape: {result[0].shape}")
        
except Exception as e:
    print("Exception:", e)
    sys.exit(1)
