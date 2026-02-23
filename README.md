<p align="center">
  <h1 align="center">🇨🇳 AiPinyin · 爱拼音</h1>
  <p align="center">AI 驱动的轻量级本地拼音输入法 · 零联网 · 零广告 · 开箱即用</p>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/language-Rust-orange?logo=rust" />
  <img src="https://img.shields.io/badge/platform-Windows-blue?logo=windows" />
  <img src="https://img.shields.io/badge/AI-ONNX_Runtime-green?logo=onnx" />
  <img src="https://img.shields.io/badge/license-Non--Commercial-yellow" />
</p>

---

## ✨ 特性

- **🧠 AI 驱动** — 内置 PinyinGPT 模型（GPT-2 架构，~13 MB ONNX），基于上下文语义预测候选词，越用越懂你
- **📖 三级词典索引** — 精确匹配 / 前缀匹配 / 首字母缩写，全部 O(1) HashMap 查找，按键响应 <1ms
- **🔌 JS 插件系统** — QuickJS 沙箱隔离，支持热加载 `.js` 插件自定义候选词处理流水线
- **🎨 主题可定制** — 通过 `style.css` CSS 变量自定义配色、字号、圆角等视觉参数
- **🛡️ 守护进程** — 后台自动监控并修复 Win11 输入法服务 (`ctfmon.exe`) 消失的问题
- **📝 自学习词典** — 自动记录用户选词习惯，持久化存储，支持撤销学习
- **🚫 无后门** — 纯本地推理，不联网，不收集数据，不弹广告

---

## 🏗️ 架构

```
┌─────────────────────────────────────────────────────────┐
│                     AiPinyin                            │
├──────────┬──────────┬───────────┬───────────┬───────────┤
│ 键盘钩子  │  拼音引擎  │  AI 引擎   │  词典系统   │  UI 窗口   │
│ WH_KEY.. │ 贪心切分   │ PinyinGPT │ 三级索引   │ Win32 GDI │
│ LL Hook  │ 歧义分析   │ ONNX 推理  │ bincode   │ 双排布局   │
├──────────┴──────────┴───────────┴───────────┴───────────┤
│  插件系统 (QuickJS)  │  用户词典  │  设置 (WebView2)  │ 守护进程 │
└──────────────────────┴───────────┴───────────────────┴──────────┘
```

### 源码模块

| 模块 | 文件 | 职责 |
|------|------|------|
| 主入口 | `main.rs` | 全局低阶键盘钩子、按键分发、候选翻页、光标定位 |
| 拼音引擎 | `pinyin.rs` | 拼音音节切分（贪心最长匹配 + 歧义备选）、三级词典索引构建与查询 |
| AI 引擎 | `ai_engine.rs` | PinyinGPT ONNX 推理、上下文感知预测、字典引导重排 |
| 候选窗口 | `ui.rs` | Win32 GDI 绘制，双排布局（拼音 + 候选词），圆角窗口，主题加载 |
| 键盘事件 | `key_event.rs` | `ITfKeyEventSink` COM 接口实现，按键→拼音→候选逻辑 |
| 插件系统 | `plugin_system.rs` | QuickJS 沙箱，插件加载/授权/槽位管理，候选词流水线处理 |
| 配置管理 | `config.rs` | `config.toml` 解析，引擎模式/AI/UI/词库参数 |
| 设置窗口 | `settings.rs` | WebView2 图形化设置界面 |
| 用户词典 | `user_dict.rs` | 选词学习/撤销学习/权重持久化 |
| 守护进程 | `guardian.rs` | `ctfmon.exe` 存活监控与自动重启 |

---

## 🧠 AI 模型

| 项目 | 说明 |
|------|------|
| 架构 | GPT-2 Chinese (12 层, 12 头, 768 维, 102.4M 参数) |
| 格式 | ONNX (`gpt2_int8.onnx`, ~13 MB / 98 MB) |
| 推理 | ONNX Runtime 动态加载，后台线程异步推理 |
| 词表 | `char2id.json` (汉字→ID) + `pinyin2id.json` (拼音→ID) |

### 双模式引擎

通过 `config.toml` 的 `engine.mode` 切换：

- **`ai` 模式**（默认）— AI 主导预测，前 K 位由 AI 生成，字典候选兜底
- **`dict` 模式** — 字典主导查表，AI 参与上下文感知重排

---

## 📚 词典系统

- **主词典** `dict.txt` — ~10 MB，格式 `拼音,汉字,权重`
- **二进制缓存** `dict.bin` — 首次加载后自动生成（bincode 序列化），后续启动秒加载
- **扩展词库** `dict/` 目录下可选词库：

| 词库 | 文件名 | 内容 |
|------|--------|------|
| IT 计算机 | `sogou_it.txt` | 编程/技术术语 |
| 日常用语 | `sogou_daily.txt` | 常用口语表达 |
| 网络新词 | `sogou_neologism.txt` | 流行网络用语 |
| 古诗词 | `sogou_poem.txt` | 古典诗词名句 |
| 城市信息 | `sogou_city.txt` | 城市与区域名称 |
| 城市地名 | `sogou_region.txt` | 省市区地名 |
| 医学词汇 | `sogou_medical.txt` | 医学专业用语 |
| 食物 | `sogou_food.txt` | 食品饮料名称 |
| 成语 | `sogou_idiom.txt` | 常用成语 |

在 `config.toml` 中启用：

```toml
[dict]
extra = ["sogou_it", "sogou_daily"]
```

---

## ⚙️ 配置

所有配置文件均位于 `aipinyin.exe` 同目录下：

### `config.toml` — 引擎配置

```toml
[engine]
mode = "ai"          # "ai" = AI主导, "dict" = 字典主导

[ai]
top_k = 5            # AI 候选数量
rerank = true        # AI 是否参与字典候选排序

[ui]
font_size = 16
opacity = 240        # 窗口透明度 (0-255)

[dict]
extra = []           # 额外加载的词库
```

### `style.css` — 视觉主题

```css
:root {
    --bg-color:        #2E313E;
    --text-color:      #C8CCD8;
    --pinyin-color:    #A9B1D6;
    --index-color:     #82869C;
    --highlight-bg:    #7AA2F7;
    --highlight-text:  #FFFFFF;
    --font-size:       24px;
    --pinyin-size:     22px;
    --corner-radius:   14px;
    --padding-h:       14px;
}
```

---

## 🔌 插件系统

将 `.js` 文件放入 `plugins/` 目录即可。插件在 QuickJS 沙箱中运行，互相隔离。

- 最多同时激活 **5** 个插件
- 首次启用需用户授权确认
- 通过候选窗口右上角 **[JS]** 按钮管理

插件接口：

```javascript
// 接收拼音和候选词数组，返回处理后的候选词
function on_candidates(pinyin, candidates) {
    // 自定义处理逻辑
    return candidates;
}
```

---

## 🔨 构建

### 环境要求

- Rust 稳定版工具链
- Windows 10/11
- ONNX Runtime（运行时动态加载）

### 编译

```bash
cargo build --release
```

编译产物位于 `target/release/aipinyin.exe`。

### 运行所需文件

将以下文件放置在 `aipinyin.exe` 同目录下：

```
aipinyin.exe
gpt2_int8.onnx        # AI 模型权重 (INT8量化版)
char2id.json          # 汉字词表
pinyin2id.json        # 拼音词表
vocab_meta.json       # 词表元数据
dict.txt              # 主词典
config.toml           # 配置文件（可选）
style.css             # 主题样式（可选）
onnxruntime.dll       # ONNX Runtime 动态库
dict/                 # 扩展词库目录（可选）
plugins/              # 插件目录（可选）
```

---

## 📄 许可证

**非商业使用许可（Non-Commercial License）**

允许个人学习、研究、使用及二次开发。禁止商业销售。详见 [LICENSE](LICENSE)。

---

<p align="center">
  <sub>让输入法重回人类逻辑。</sub>
</p>
