"""
AiPinyin 训练脚本

针对 RTX 3060 (6GB VRAM) 优化:
  - FP16 混合精度训练 (torch.cuda.amp)
  - 梯度累积 (effective batch = batch_size × accum_steps)
  - 自动保存 checkpoint

用法:
  python trainer/train.py --data trainer/data/corpus.txt --epochs 50

数据格式 (corpus.txt):
  每行一个句子, 用空格分隔拼音:
  ni hao shi jie\t你好世界

  即: 拼音序列 \\t 汉字序列
"""

import os
import sys
import json
import argparse
import time
from typing import List, Tuple

import torch
import torch.nn as nn
from torch.utils.data import Dataset, DataLoader, RandomSampler
# AMP: 兼容新旧 API
try:
    from torch.amp import autocast, GradScaler  # PyTorch 2.9+
except ImportError:
    from torch.cuda.amp import autocast, GradScaler

# 添加 trainer 目录到 path
sys.path.insert(0, os.path.dirname(__file__))
from tokenizer import PinyinTokenizer, CharTokenizer, BOS_ID, EOS_ID, PAD_ID
from model import PinyinTransformer, create_model


# ============================================================
# 数据集
# ============================================================

class PinyinDataset(Dataset):
    """
    每条样本:
      输入: context_ids (前文汉字) + pinyin_ids (当前拼音)
      标签: 当前拼音对应的汉字 ID

    滑动窗口: 一个句子生成多条训练样本
    """

    def __init__(
        self,
        data_file: str,
        py_tok: PinyinTokenizer,
        ch_tok: CharTokenizer,
        max_pinyin_len: int = 32,
        max_context_len: int = 64,
    ):
        self.py_tok = py_tok
        self.ch_tok = ch_tok
        self.max_py = max_pinyin_len
        self.max_ctx = max_context_len
        self.samples: List[Tuple[List[int], List[int], int]] = []
        self._load(data_file)

    def _load(self, path: str):
        if not os.path.exists(path):
            print(f"[Train] ⚠ 数据文件不存在: {path}")
            print(f"[Train] 使用内置示例数据...")
            self._load_demo()
            return

        count = 0
        with open(path, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line or "\t" not in line:
                    continue
                py_str, char_str = line.split("\t", 1)
                syllables = py_str.strip().split()
                chars = list(char_str.strip())

                if len(syllables) != len(chars):
                    continue

                self._make_samples(syllables, chars)
                count += 1

        print(f"[Train] 从 {path} 加载 {count} 个句子, 生成 {len(self.samples)} 个训练样本")

    def _load_demo(self):
        """内置示例数据"""
        demo = [
            ("ni hao", "你好"),
            ("shi jie", "世界"),
            ("zhong guo", "中国"),
            ("wo men", "我们"),
            ("jin tian tian qi hen hao", "今天天气很好"),
            ("xie xie ni", "谢谢你"),
            ("zai jian", "再见"),
            ("da jia hao", "大家好"),
        ]
        for py_str, char_str in demo:
            syllables = py_str.split()
            chars = list(char_str)
            if len(syllables) == len(chars):
                self._make_samples(syllables, chars)
        print(f"[Train] 使用内置示例: {len(self.samples)} 个训练样本")

    def _make_samples(self, syllables: List[str], chars: List[str]):
        """滑动窗口生成训练样本"""
        for i in range(len(chars)):
            # 上下文: 前面的汉字
            context = chars[:i]
            ctx_ids = self.ch_tok.encode("".join(context))

            # 截断 + padding 上下文
            if len(ctx_ids) > self.max_ctx:
                ctx_ids = ctx_ids[-self.max_ctx:]

            # 当前拼音 (可以是单字或多字)
            py_ids = self.py_tok.encode([syllables[i]])

            # 目标: 当前汉字
            target = self.ch_tok.char2id.get(chars[i], 1)  # UNK=1

            self.samples.append((ctx_ids, py_ids, target))

    def __len__(self):
        return len(self.samples)

    def __getitem__(self, idx):
        ctx_ids, py_ids, target = self.samples[idx]

        # Padding
        ctx_padded = ctx_ids + [PAD_ID] * (self.max_ctx - len(ctx_ids))
        py_padded = py_ids + [PAD_ID] * (self.max_py - len(py_ids))

        return (
            torch.tensor(ctx_padded[:self.max_ctx], dtype=torch.long),
            torch.tensor(py_padded[:self.max_py], dtype=torch.long),
            torch.tensor(target, dtype=torch.long),
        )


# ============================================================
# 训练循环
# ============================================================

def train(args):
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    print(f"[Train] Device: {device}")
    if device.type == "cuda":
        print(f"[Train] GPU: {torch.cuda.get_device_name()}")
        print(f"[Train] VRAM: {torch.cuda.get_device_properties(0).total_memory / 1e9:.1f} GB")

    # Tokenizer
    vocab_dir = os.path.join(os.path.dirname(__file__), "vocab")
    py_tok = PinyinTokenizer()
    ch_tok = CharTokenizer()

    # 保存词表
    os.makedirs(vocab_dir, exist_ok=True)
    py_tok.save(os.path.join(vocab_dir, "pinyin2id.json"))
    ch_tok.save(os.path.join(vocab_dir, "char2id.json"))
    meta = {
        "pinyin_vocab_size": py_tok.vocab_size,
        "char_vocab_size": ch_tok.vocab_size,
        "d_model": 256, "nhead": 4, "num_layers": 6,
        "max_pinyin_len": 32, "max_context_len": 64,
    }
    with open(os.path.join(vocab_dir, "vocab_meta.json"), "w") as f:
        json.dump(meta, f, indent=2)

    # 数据集
    dataset = PinyinDataset(args.data, py_tok, ch_tok)
    if len(dataset) == 0:
        print("[Train] ❌ 没有训练数据!")
        return

    # 限制每 epoch 样本数
    if args.max_samples > 0 and len(dataset) > args.max_samples:
        sampler = RandomSampler(dataset, num_samples=args.max_samples)
        print(f"[Train] 限制每 epoch {args.max_samples:,} 样本 (总共 {len(dataset):,})", flush=True)
    else:
        sampler = None

    loader = DataLoader(
        dataset,
        batch_size=args.batch_size,
        shuffle=(sampler is None),
        sampler=sampler,
        num_workers=2,
        pin_memory=True,
    )

    # 模型
    model = create_model(py_tok.vocab_size, ch_tok.vocab_size).to(device)
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=0.01)
    criterion = nn.CrossEntropyLoss(ignore_index=PAD_ID)
    use_amp = device.type == "cuda"
    scaler = GradScaler(device.type, enabled=use_amp) if use_amp else None

    # 学习率调度
    scheduler = torch.optim.lr_scheduler.CosineAnnealingLR(
        optimizer, T_max=args.epochs, eta_min=1e-6
    )

    # Checkpoint
    os.makedirs(args.save_dir, exist_ok=True)
    best_loss = float("inf")

    print(f"\n[Train] 开始训练: {args.epochs} epochs, batch={args.batch_size}", flush=True)
    print(f"[Train] 样本数: {len(dataset)}, 批次数: {len(loader)}", flush=True)
    print(f"[Train] AMP(FP16): {use_amp}", flush=True)
    print(flush=True)

    for epoch in range(1, args.epochs + 1):
        model.train()
        total_loss = 0.0
        correct = 0
        total = 0
        t0 = time.time()

        for batch_idx, (ctx, py, target) in enumerate(loader):
            ctx = ctx.to(device)
            py = py.to(device)
            target = target.to(device)

            if use_amp:
                with autocast(device.type):
                    logits = model(py, ctx)
                    loss = criterion(logits, target)
                scaler.scale(loss).backward()
                if (batch_idx + 1) % args.accum_steps == 0:
                    scaler.step(optimizer)
                    scaler.update()
                    optimizer.zero_grad()
            else:
                logits = model(py, ctx)
                loss = criterion(logits, target)
                loss.backward()
                if (batch_idx + 1) % args.accum_steps == 0:
                    optimizer.step()
                    optimizer.zero_grad()

            total_loss += loss.item()
            preds = logits.argmax(dim=-1)
            correct += (preds == target).sum().item()
            total += target.size(0)

            # 每 100 批次打印进度
            if (batch_idx + 1) % 100 == 0:
                running_loss = total_loss / (batch_idx + 1)
                running_acc = correct / max(total, 1)
                elapsed = time.time() - t0
                eta = elapsed / (batch_idx + 1) * (len(loader) - batch_idx - 1)
                print(f"  [{batch_idx+1:6d}/{len(loader)}]  "
                      f"loss={running_loss:.4f}  acc={running_acc:.3f}  "
                      f"elapsed={elapsed:.0f}s  eta={eta:.0f}s", flush=True)

        scheduler.step()

        avg_loss = total_loss / len(loader)
        acc = correct / max(total, 1)
        elapsed = time.time() - t0
        lr = optimizer.param_groups[0]["lr"]

        print(f"Epoch {epoch:3d}/{args.epochs}  "
              f"loss={avg_loss:.4f}  acc={acc:.3f}  "
              f"lr={lr:.2e}  time={elapsed:.1f}s")

        # 保存最优
        if avg_loss < best_loss:
            best_loss = avg_loss
            ckpt_path = os.path.join(args.save_dir, "best_model.pt")
            torch.save({
                "epoch": epoch,
                "model_state_dict": model.state_dict(),
                "optimizer_state_dict": optimizer.state_dict(),
                "loss": avg_loss,
                "pinyin_vocab": py_tok.vocab_size,
                "char_vocab": ch_tok.vocab_size,
            }, ckpt_path)
            print(f"  → 保存最优模型 (loss={avg_loss:.4f})")

    # 最终保存
    final_path = os.path.join(args.save_dir, "final_model.pt")
    torch.save(model.state_dict(), final_path)
    print(f"\n[Train] 训练完成! 最终模型: {final_path}")
    print(f"[Train] 最优模型: {os.path.join(args.save_dir, 'best_model.pt')}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="AiPinyin Transformer 训练")
    parser.add_argument("--data", type=str, default="trainer/data/corpus.txt",
                        help="训练数据文件 (拼音\\t汉字)")
    parser.add_argument("--epochs", type=int, default=50)
    parser.add_argument("--batch_size", type=int, default=64,
                        help="RTX 3060 6GB 建议 64")
    parser.add_argument("--lr", type=float, default=3e-4)
    parser.add_argument("--accum_steps", type=int, default=4,
                        help="梯度累积步数 (有效 batch = batch_size × accum_steps)")
    parser.add_argument("--save_dir", type=str, default="trainer/checkpoints")
    parser.add_argument("--max_samples", type=int, default=500000,
                        help="每 epoch 最大样本数 (0=全部)")
    args = parser.parse_args()
    train(args)
