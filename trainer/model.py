"""
AiPinyin Transformer-Encoder 模型

架构:
  拼音序列 ──┐
              ├─→ Concat → Transformer-Encoder (6层) → 分类头 → 汉字 Logits
  上下文汉字 ─┘

规格:
  - d_model = 256
  - nhead = 4
  - num_layers = 6
  - dim_feedforward = 1024
  - dropout = 0.1
  - 参数量 ≈ 25M → FP32 约 100MB

输入:
  - pinyin_ids:  [batch, pinyin_len]   LongTensor  拼音音节 ID
  - context_ids: [batch, context_len]  LongTensor  上下文汉字 ID

输出:
  - logits: [batch, char_vocab_size]   FloatTensor 每个汉字的概率
"""

import math
import torch
import torch.nn as nn


class PositionalEncoding(nn.Module):
    """标准正弦位置编码"""

    def __init__(self, d_model: int, max_len: int = 512, dropout: float = 0.1):
        super().__init__()
        self.dropout = nn.Dropout(dropout)

        pe = torch.zeros(max_len, d_model)
        position = torch.arange(0, max_len, dtype=torch.float).unsqueeze(1)
        div_term = torch.exp(
            torch.arange(0, d_model, 2).float() * (-math.log(10000.0) / d_model)
        )
        pe[:, 0::2] = torch.sin(position * div_term)
        pe[:, 1::2] = torch.cos(position * div_term)
        pe = pe.unsqueeze(0)  # [1, max_len, d_model]
        self.register_buffer("pe", pe)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        # x: [batch, seq_len, d_model]
        x = x + self.pe[:, :x.size(1), :]
        return self.dropout(x)


class PinyinTransformer(nn.Module):
    """拼音→汉字 Transformer-Encoder 模型"""

    def __init__(
        self,
        pinyin_vocab_size: int = 415,
        char_vocab_size: int = 7020,
        d_model: int = 256,
        nhead: int = 4,
        num_layers: int = 6,
        dim_feedforward: int = 1024,
        dropout: float = 0.1,
        max_pinyin_len: int = 32,
        max_context_len: int = 64,
    ):
        super().__init__()
        self.d_model = d_model
        self.char_vocab_size = char_vocab_size

        # 嵌入层
        self.pinyin_embed = nn.Embedding(pinyin_vocab_size, d_model, padding_idx=0)
        self.char_embed = nn.Embedding(char_vocab_size, d_model, padding_idx=0)

        # 类型嵌入 (0=context, 1=pinyin) — 区分两种输入
        self.type_embed = nn.Embedding(2, d_model)

        # 位置编码
        self.pos_encoder = PositionalEncoding(
            d_model, max_len=max_pinyin_len + max_context_len, dropout=dropout
        )

        # Transformer Encoder
        encoder_layer = nn.TransformerEncoderLayer(
            d_model=d_model,
            nhead=nhead,
            dim_feedforward=dim_feedforward,
            dropout=dropout,
            batch_first=True,  # [batch, seq, feature]
            activation="gelu",
        )
        self.encoder = nn.TransformerEncoder(encoder_layer, num_layers=num_layers)

        # 分类头: 取 [BOS] 位置的特征 → 汉字概率
        self.classifier = nn.Sequential(
            nn.LayerNorm(d_model),
            nn.Linear(d_model, dim_feedforward),
            nn.GELU(),
            nn.Dropout(dropout),
            nn.Linear(dim_feedforward, char_vocab_size),
        )

        self._init_weights()

    def _init_weights(self):
        for p in self.parameters():
            if p.dim() > 1:
                nn.init.xavier_uniform_(p)

    def forward(
        self,
        pinyin_ids: torch.Tensor,    # [batch, pinyin_len]
        context_ids: torch.Tensor,   # [batch, context_len]
    ) -> torch.Tensor:
        """
        Returns: logits [batch, char_vocab_size]
        """
        batch_size = pinyin_ids.size(0)

        # 嵌入
        ctx_emb = self.char_embed(context_ids)  # [B, ctx_len, D]
        py_emb = self.pinyin_embed(pinyin_ids)   # [B, py_len, D]

        # 类型标记
        ctx_type = torch.zeros(context_ids.size(), dtype=torch.long, device=context_ids.device)
        py_type = torch.ones(pinyin_ids.size(), dtype=torch.long, device=pinyin_ids.device)
        ctx_emb = ctx_emb + self.type_embed(ctx_type)
        py_emb = py_emb + self.type_embed(py_type)

        # 拼接: [context, pinyin]
        combined = torch.cat([ctx_emb, py_emb], dim=1)  # [B, ctx+py, D]

        # 位置编码
        combined = self.pos_encoder(combined)

        # Padding mask: 0 位置为 True (被 mask)
        ctx_mask = (context_ids == 0)  # [B, ctx_len]
        py_mask = (pinyin_ids == 0)    # [B, py_len]
        src_key_padding_mask = torch.cat([ctx_mask, py_mask], dim=1)  # [B, ctx+py]

        # Transformer 编码
        encoded = self.encoder(
            combined, src_key_padding_mask=src_key_padding_mask
        )  # [B, ctx+py, D]

        # 取第一个 token (BOS 位置) 的特征做分类
        # 如果没有 BOS, 取平均池化
        # 这里简单取第一个非 padding 位置
        first_token = encoded[:, 0, :]  # [B, D]

        # 分类
        logits = self.classifier(first_token)  # [B, char_vocab_size]

        return logits

    def count_parameters(self) -> int:
        return sum(p.numel() for p in self.parameters() if p.requires_grad)


def create_model(pinyin_vocab: int = 415, char_vocab: int = 7020) -> PinyinTransformer:
    """创建默认配置的模型"""
    model = PinyinTransformer(
        pinyin_vocab_size=pinyin_vocab,
        char_vocab_size=char_vocab,
    )
    params = model.count_parameters()
    size_mb = params * 4 / 1024 / 1024  # FP32
    print(f"[Model] PinyinTransformer: {params:,} params ({size_mb:.1f} MB FP32)")
    return model


if __name__ == "__main__":
    model = create_model()
    print(model)

    # 测试前向传播
    batch = 2
    pinyin = torch.randint(0, 415, (batch, 8))
    context = torch.randint(0, 7020, (batch, 16))
    logits = model(pinyin, context)
    print(f"\n输入: pinyin {pinyin.shape}, context {context.shape}")
    print(f"输出: logits {logits.shape}")
    print(f"Top-5 预测: {torch.topk(logits[0], 5)}")
