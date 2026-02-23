<p align="center">
  <h1 align="center">🇨🇳 AiPinyin · 爱拼音</h1>
  <p align="center">AI 驱动的轻量级本地拼音输入法 · 零联网 · 零广告 · 开箱即用</p>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/language-Rust-orange?logo=rust" />
  <img src="https://img.shields.io/badge/platform-Windows-blue?logo=windows" />
  <img src="https://img.shields.io/badge/AI-ONNX_Runtime-green?logo=onnx" />
  <img src="https://img.shields.io/badge/model-HuggingFace-yellow?logo=huggingface" />
  <img src="https://img.shields.io/badge/license-Non--Commercial-lightgrey" />
</p>

---

## ✨ 特性

- **🧠 AI 驱动** — 内置 GPT2-Chinese 量化模型（INT8 ONNX，~99 MB），基于上下文语义预测候选词，越用越懂你
- **🌐 OpenAI 兼容接口** — 本地内嵌 HTTP 服务，`POST /v1/chat/completions`，可无缝替换为 Ollama / ChatGPT / 任意兼容后端
- **🎨 UI 主题市场** — 候选窗口基于 WebView2，`ui/` 目录下 HTML/CSS/JS 完全可替换，支持远程主题 URL
- **🔌 JS 插件系统** — QuickJS 沙箱隔离，支持热加载 `.js` 插件自定义候选词处理流水线
- **📖 三级词典索引** — 精确匹配 / 前缀匹配 / 首字母缩写，全部 O(1) HashMap 查找，按键响应 <1ms
- **📝 自学习词典** — 自动记录用户选词习惯，持久化存储，支持退格撤销学习
- **🛡️ 守护进程** — 后台监控并自动修复 Win11 输入法服务消失的问题
- **🚫 无后门** — 纯本地推理，不联网，不收集数据，不弹广告

---

## 🏗️ 架构

```
aipinyin.exe
├── 键盘钩子 (WH_KEYBOARD_LL)          全局低阶钩子，捕获所有按键
├── 拼音引擎 (pinyin.rs)               贪心音节切分 + 三级词典索引
├── 本地 AI HTTP 服务 (ai_server.rs)   OpenAI 兼容接口 (localhost:876x)
│   ├── POST /v1/chat/completions      AI 推理接口
│   ├── GET  /ui/*                     UI 静态文件服务（支持主题热替换）
│   └── GET  /v1/status               健康检查
├── AI 引擎 (ai_engine.rs)            GPT2-Chinese ONNX 推理 + Beam Search
├── WebView2 候选窗口 (webview_ui.rs)  加载本地 http://127.0.0.1:{port}/ui/
└── JS 插件系统 (plugin_system.rs)    QuickJS 沙箱，候选词流水线
```

### 源码模块

| 模块 | 文件 | 职责 |
|------|------|------|
| 主入口 | `main.rs` | 钩子、按键分发、候选翻页、光标定位 |
| 拼音引擎 | `pinyin.rs` | 音节切分、三级词典索引构建与查询 |
| AI 引擎 | `ai_engine.rs` | GPT2 ONNX 推理、上下文感知预测、Beam Search |
| AI HTTP 服务 | `ai_server.rs` | OpenAI 兼容接口 + UI 静态文件服务 |
| 候选窗口 | `webview_ui.rs` | WebView2 透明窗口，IPC 通信，主题加载 |
| 键盘事件 | `key_event.rs` | 按键→拼音→候选逻辑 |
| 插件系统 | `plugin_system.rs` | QuickJS 沙箱，插件加载/授权/管理 |
| 配置管理 | `config.rs` | `config.toml` 解析 |
| 设置界面 | `settings.rs` | WebView2 图形化设置 |
| 用户词典 | `user_dict.rs` | 选词学习/撤销/权重持久化 |
| 守护进程 | `guardian.rs` | `ctfmon.exe` 存活监控与自动重启 |

---

## 🧠 AI 模型

| 项目 | 说明 |
|------|------|
| 架构 | GPT-2 Chinese（12 层，12 头，768 维，102M 参数） |
| 格式 | ONNX INT8 量化，`gpt2_int8.onnx`，~99 MB |
| 推理 | ONNX Runtime，后台线程异步，单次推理 <5ms |
| 下载 | 🤗 [tang30000/AiPinyin-gpt2chinese](https://huggingface.co/tang30000/AiPinyin-gpt2chinese) |

> 模型文件较大（~99 MB），托管于 Hugging Face，不包含在 git 仓库中。

### 切换 AI 后端

在 `config.toml` 中配置，留空使用内置本地模型：

```toml
[ai]
endpoint = ""                          # 空 = 本地 GPT2 兜底（默认）
# endpoint = "http://localhost:11434/v1"  # Ollama
# endpoint = "https://api.openai.com/v1" # ChatGPT
api_key  = ""                          # 外部服务的 API Key
```

---

## 📚 词典系统

- **主词典** `dict.txt` — ~10 MB，格式 `拼音,汉字,权重`
- **二进制缓存** `dict.bin` — 首次加载自动生成（bincode 序列化），后续秒级启动
- **扩展词库** 放置于 `dict/` 目录，在 `config.toml` 中启用：

```toml
[dict]
extra = ["sogou_it", "sogou_daily", "sogou_neologism"]
```

| 词库名 | 内容 |
|--------|------|
| `sogou_common` | 通用词汇（基础，建议开启） |
| `sogou_it` | IT/编程术语 |
| `sogou_daily` | 日常口语 |
| `sogou_city` | 城市信息 |
| `sogou_neologism` | 网络新词 |
| `sogou_poem` | 古诗词 |
| `sogou_medical` | 医学专业词汇 |
| `sogou_food` | 食品饮料 |
| `sogou_idiom` | 成语 |

---

## 🎨 UI 主题定制

候选窗口由 WebView2 渲染，修改 `ui/` 目录下的文件即可定制外观：

- `ui/index.html` — 结构
- `ui/style.css` — 样式（CSS 变量控制配色、字号、圆角等）
- `ui/script.js` — 交互逻辑

也可在 `config.toml` 中配置远程主题 URL（将来支持主题市场）：

```toml
[ui]
# url = "https://example.com/my-theme/index.html"
```

---

## ⚙️ 配置参考

```toml
[engine]
mode = "ai"          # "ai" = AI 主导，"dict" = 字典主导

[ai]
top_k = 9            # AI 候选数量
rerank = true        # AI 是否参与字典候选排序
endpoint = ""        # 外部 AI 接口（空 = 本地兜底）
api_key  = ""        # 外部服务 API Key
system_prompt = ""   # 自定义 AI 系统提示词（空 = 内置中文提示词）

[ui]
font_size = 16
opacity = 240        # 窗口透明度 (0-255)

[dict]
extra = ["sogou_common", "sogou_daily"]
```

---

## 🔌 插件系统

将 `.js` 文件放入 `plugins/` 目录即可。插件在 QuickJS 沙箱中运行：

```javascript
// 接收拼音和候选词，返回处理后的候选词
function on_candidates(pinyin, candidates) {
    return candidates;
}
```

- 最多同时激活 **5** 个插件
- 首次启用需用户授权
- 通过候选窗口右上角 **[JS]** 按钮管理

---

## 🔨 构建

### 环境要求

- Rust 稳定版（推荐 1.75+）
- Windows 10 / 11
- WebView2 Runtime（Win11 内置，Win10 需单独安装）
- ONNX Runtime（`onnxruntime.dll`，运行时动态加载）

### 编译

```bash
cargo build --release
```

### 准备运行所需文件

```
aipinyin.exe
onnxruntime.dll          # ONNX Runtime 动态库
gpt2_int8.onnx           # 模型文件（从 HuggingFace 下载）
char2id.json             # 汉字词表
pinyin2id.json           # 拼音词表
vocab_meta.json          # 词表元数据
dict.txt                 # 主词典
ui/                      # 候选窗口 UI（HTML/CSS/JS）
config.toml              # 配置（可选）
dict/                    # 扩展词库（可选）
plugins/                 # 插件目录（可选）
```

**下载模型：**

```bash
# 方法一：直接下载
curl -L https://huggingface.co/tang30000/AiPinyin-gpt2chinese/resolve/main/gpt2_int8.onnx -o gpt2_int8.onnx

# 方法二：huggingface-cli
huggingface-cli download tang30000/AiPinyin-gpt2chinese gpt2_int8.onnx --local-dir .
```

---

## 📄 许可证

**非商业使用许可（Non-Commercial License）**

允许个人学习、研究、使用及二次开发。禁止商业销售。详见 [LICENSE](LICENSE)。

---

<p align="center">
  <sub>让输入法重回人类逻辑。</sub>
</p>
