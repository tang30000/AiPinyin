//! # 设置窗口 (WebView2)
//!
//! 使用 wry + tao 创建 WebView2 窗口，加载 settings.html。
//! 配置数据在加载时注入 HTML，IPC 仅用于 save/toggle/delete。

use std::path::PathBuf;

/// 获取 exe 所在目录
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 读取当前配置和样式，返回 JSON 字符串
fn load_config_json() -> String {
    let dir = exe_dir();

    // 读 config.toml
    let config_path = dir.join("config.toml");
    let config_text = std::fs::read_to_string(&config_path).unwrap_or_default();
    let config: toml::Value = config_text.parse().unwrap_or(toml::Value::Table(Default::default()));

    let engine_mode = config.get("engine").and_then(|e| e.get("mode"))
        .and_then(|v| v.as_str()).unwrap_or("ai");
    let top_k = config.get("ai").and_then(|a| a.get("top_k"))
        .and_then(|v| v.as_integer()).unwrap_or(5);
    let rerank = config.get("ai").and_then(|a| a.get("rerank"))
        .and_then(|v| v.as_bool()).unwrap_or(true);
    let opacity = config.get("ui").and_then(|u| u.get("opacity"))
        .and_then(|v| v.as_integer()).unwrap_or(240);
    let extra: Vec<String> = config.get("dict").and_then(|d| d.get("extra"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    // 读 style.css → 解析 CSS 变量
    let style_path = dir.join("style.css");
    let css = std::fs::read_to_string(&style_path).unwrap_or_default();
    let parse_css_var = |name: &str, default: &str| -> String {
        css.lines()
            .find(|line| line.contains(name))
            .and_then(|line| {
                let start = line.find(':')?;
                let end = line.find(';')?;
                Some(line[start+1..end].trim().to_string())
            })
            .unwrap_or_else(|| default.to_string())
    };

    let bg_color = parse_css_var("--bg-color", "#2E313E");
    let text_color = parse_css_var("--text-color", "#C8CCD8");
    let pinyin_color = parse_css_var("--pinyin-color", "#6E738C");
    let index_color = parse_css_var("--index-color", "#82869C");
    let highlight_bg = parse_css_var("--highlight-bg", "#7AA2F7");
    let highlight_text = parse_css_var("--highlight-text", "#FFFFFF");
    let font_size = parse_css_var("--font-size", "20px");
    let pinyin_size = parse_css_var("--pinyin-size", "20px");
    let corner_radius = parse_css_var("--corner-radius", "14px");

    // 读 plugins/
    let plugins_dir = dir.join("plugins");
    let authorized = std::fs::read_to_string(plugins_dir.join(".authorized")).unwrap_or_default();
    let plugins: Vec<String> = if plugins_dir.exists() {
        std::fs::read_dir(&plugins_dir).ok()
            .map(|entries| entries.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|ext| ext == "js").unwrap_or(false))
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let enabled = authorized.lines().any(|l| l.trim() == name);
                    format!(r#"{{"name":"{}","enabled":{}}}"#, name, enabled)
                })
                .collect())
            .unwrap_or_default()
    } else { vec![] };

    let extra_json: Vec<String> = extra.iter().map(|s| format!("\"{}\"", s)).collect();

    format!(r#"{{
  "config": {{
    "engine_mode": "{}",
    "top_k": {},
    "rerank": {},
    "opacity": {},
    "extra": [{}]
  }},
  "style": {{
    "bg_color": "{}",
    "text_color": "{}",
    "pinyin_color": "{}",
    "index_color": "{}",
    "highlight_bg": "{}",
    "highlight_text": "{}",
    "font_size": "{}",
    "pinyin_size": "{}",
    "corner_radius": "{}"
  }},
  "plugins": [{}]
}}"#,
        engine_mode, top_k, rerank, opacity, extra_json.join(","),
        bg_color, text_color, pinyin_color, index_color,
        highlight_bg, highlight_text, font_size, pinyin_size, corner_radius,
        plugins.join(","))
}

/// 保存 config.toml
fn save_config(data: &serde_json::Value) {
    let dir = exe_dir();
    let config = &data["config"];

    let engine_mode = config["engine_mode"].as_str().unwrap_or("ai");
    let top_k = config["top_k"].as_i64().unwrap_or(5);
    let rerank = config["rerank"].as_bool().unwrap_or(true);
    let opacity = config["opacity"].as_i64().unwrap_or(240);
    let extra: Vec<&str> = config["extra"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let extra_str: Vec<String> = extra.iter().map(|s| format!("\"{}\"", s)).collect();

    let toml_content = format!(
r#"# AiPinyin 配置文件
# 放置于 aipinyin.exe 同目录

[engine]
mode = "{}"

[ai]
top_k = {}
rerank = {}

[ui]
font_size = {}
opacity = {}

[dict]
extra = [{}]
"#, engine_mode, top_k, rerank, top_k, opacity, extra_str.join(", "));

    let _ = std::fs::write(dir.join("config.toml"), toml_content);
    eprintln!("[Settings] ✅ config.toml 已保存");
}

/// 保存 style.css
fn save_style(data: &serde_json::Value) {
    let dir = exe_dir();
    let s = &data["style"];

    let css = format!(
r#"/* AiPinyin 候选词窗口样式表
 *
 * 修改此文件即可自定义外观，无需重新编译。
 * 重启 AiPinyin 后生效。
 */

:root {{
    --bg-color: {};
    --text-color: {};
    --pinyin-color: {};
    --index-color: {};
    --highlight-bg: {};
    --highlight-text: {};
    --font-size: {};
    --pinyin-size: {};
    --corner-radius: {};
    --padding-h: 14px;
}}
"#,
        s["bg_color"].as_str().unwrap_or("#2E313E"),
        s["text_color"].as_str().unwrap_or("#C8CCD8"),
        s["pinyin_color"].as_str().unwrap_or("#6E738C"),
        s["index_color"].as_str().unwrap_or("#82869C"),
        s["highlight_bg"].as_str().unwrap_or("#7AA2F7"),
        s["highlight_text"].as_str().unwrap_or("#FFFFFF"),
        s["font_size"].as_str().unwrap_or("20px"),
        s["pinyin_size"].as_str().unwrap_or("20px"),
        s["corner_radius"].as_str().unwrap_or("14px"));

    let _ = std::fs::write(dir.join("style.css"), css);
    eprintln!("[Settings] ✅ style.css 已保存");
}

/// 删除插件文件
fn delete_plugin(name: &str) {
    let path = exe_dir().join("plugins").join(name);
    if path.exists() {
        let _ = std::fs::remove_file(&path);
        eprintln!("[Settings] 🗑 删除插件: {}", name);
    }
}

/// 切换插件启用状态
fn toggle_plugin(name: &str, enabled: bool) {
    let dir = exe_dir().join("plugins");
    let auth_path = dir.join(".authorized");
    let mut lines: Vec<String> = std::fs::read_to_string(&auth_path)
        .unwrap_or_default()
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != name)
        .collect();
    if enabled {
        lines.push(name.to_string());
    }
    let _ = std::fs::write(&auth_path, lines.join("\n"));
    eprintln!("[Settings] {} 插件: {} = {}", if enabled { "✅" } else { "❌" }, name, enabled);
}

/// 在新线程中打开设置窗口
pub fn open_settings() {
    std::thread::spawn(|| {
        if let Err(e) = open_settings_inner() {
            eprintln!("[Settings] ❌ 打开设置窗口失败: {}", e);
        }
    });
}

fn open_settings_inner() -> Result<(), Box<dyn std::error::Error>> {
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tao::platform::windows::EventLoopBuilderExtWindows;
    use tao::platform::run_return::EventLoopExtRunReturn;
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    let mut event_loop = EventLoopBuilder::new().with_any_thread(true).build();
    let window = WindowBuilder::new()
        .with_title("AiPinyin 设置")
        .with_inner_size(tao::dpi::LogicalSize::new(520.0, 720.0))
        .with_resizable(true)
        .build(&event_loop)?;

    // 加载 HTML 并注入配置数据
    let html_path = exe_dir().join("settings.html");
    let mut html_content = std::fs::read_to_string(&html_path)
        .unwrap_or_else(|_| "<h1>找不到 settings.html</h1>".into());

    // 在 </body> 前注入初始化脚本
    let config_json = load_config_json();
    let init_script = format!(
        "\n<script>window.__INIT_CONFIG__ = {};</script>\n",
        config_json
    );
    html_content = html_content.replace("</body>", &format!("{}</body>", init_script));

    let webview = WebViewBuilder::new()
        .with_html(&html_content)
        .with_ipc_handler(move |msg| {
            let body = msg.body();
            match serde_json::from_str::<serde_json::Value>(body) {
                Ok(data) => {
                    let action = data["action"].as_str().unwrap_or("");
                    match action {
                        "save" => {
                            save_config(&data);
                            save_style(&data);
                        }
                        "toggle_plugin" => {
                            if let Some(name) = data["name"].as_str() {
                                let enabled = data["enabled"].as_bool().unwrap_or(false);
                                toggle_plugin(name, enabled);
                            }
                        }
                        "delete_plugin" => {
                            if let Some(name) = data["name"].as_str() {
                                delete_plugin(name);
                            }
                        }
                        _ => {}
                    }
                }
                Err(e) => eprintln!("[Settings] IPC parse error: {}", e),
            }
        })
        .build(&window)?;

    // 防止 webview 被 drop
    let _webview = webview;

    event_loop.run_return(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            *control_flow = ControlFlow::Exit;
        }
    });

    eprintln!("[Settings] 设置窗口已关闭");
    Ok(())
}
