咱们这个项目就叫 "AiPinyin" (中文+爱拼音输入法)。我们的目标是：轻量化、本地推理、零配置、开源通用。

🛠️ 项目系统架构方案
这个方案的核心是放弃传统的“词库查表”，改用 Transformer-Lite (轻量级模型) 做拼音到汉字的概率映射。

1. 技术栈选型
核心引擎: 基于 C++/Rust 开发 Windows TSF (Text Services Framework) 接口，确保系统级稳定性。

推理库: 使用 ONNX Runtime 或 llama.cpp，专门加载咱们训练好的权重。

权重模型: 采用 Tiny-RWKV 或 Mamba-Tiny (约 45M 参数)，仅保留中文拼音预测权重，确保在 Win11 下秒开。

2. 功能模块
SmartLoader: 开机自动检测 ctfmon.exe，如果掉线自动“还魂”。

ZeroConfig UI: 界面只有字体大小、皮肤切换，没有任何 YAML 或复杂的配置文件。

LocalBrain: 纯本地运行，不联网，不推广告，不塞小方块。