"""下载 Duyu/Pinyin-Hanzi 数据集并转换为训练格式"""
import os
from huggingface_hub import hf_hub_download

out_dir = os.path.join(os.path.dirname(__file__), "data")
os.makedirs(out_dir, exist_ok=True)

print("正在从 HuggingFace 下载 Duyu/Pinyin-Hanzi ...")
path = hf_hub_download(
    repo_id="Duyu/Pinyin-Hanzi",
    filename="pinyin2hanzi.csv",
    repo_type="dataset",
    local_dir=out_dir,
)
print(f"下载完成: {path}")

# 查看前几行以确定格式
print("\n前 10 行:")
with open(path, "r", encoding="utf-8") as f:
    for i, line in enumerate(f):
        if i >= 10:
            break
        print(f"  [{i}] {line.rstrip()}")

# 统计行数
with open(path, "r", encoding="utf-8") as f:
    total = sum(1 for _ in f)
print(f"\n总行数: {total:,}")
