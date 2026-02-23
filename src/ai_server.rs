//! # 本地 AI + UI HTTP 服务 — OpenAI 兼容接口
//!
//! 单端口同时支持两类请求：
//! - `POST /v1/chat/completions`：AI 推理（OpenAI 格式，与 Ollama/LMStudio 一致）
//! - `GET  /ui/*`：静态 UI 文件（index.html / style.css / script.js 等）
//! - `GET  /v1/status`：健康检查
//!
//! 启动时自动从 8760 起寻找空闲端口，返回实际端口号。

use std::sync::{Arc, Mutex};
use std::io::Read;
use serde::{Deserialize, Serialize};
use crate::ai_engine::{AIPredictor, HistoryBuffer};

// ============================================================
// OpenAI 格式结构体
// ============================================================

#[derive(Debug, Deserialize)]
struct ChatRequest {
    #[allow(dead_code)]
    model: Option<String>,
    messages: Vec<ChatMessage>,
    #[serde(default = "default_max_tokens")]
    max_tokens: usize,
}
fn default_max_tokens() -> usize { 9 }

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatResponse {
    id: String,
    object: &'static str,
    model: &'static str,
    choices: Vec<Choice>,
}

#[derive(Serialize)]
struct Choice {
    index: usize,
    message: ChatMessage,
    finish_reason: &'static str,
}

// ============================================================
// 启动服务
// ============================================================

/// 启动本地服务，返回实际绑定端口（0 = 失败）。
pub fn start(
    predictor: Arc<Mutex<AIPredictor>>,
    history: Arc<Mutex<HistoryBuffer>>,
    ui_dir: Option<std::path::PathBuf>,
    _system_prompt: String,
) -> u16 {
    let server = (0u16..40).find_map(|i| {
        let port = 8760 + i;
        tiny_http::Server::http(format!("127.0.0.1:{}", port))
            .ok()
            .map(|s| (s, port))
    });

    let (server, port) = match server {
        Some(s) => s,
        None => {
            eprintln!("[AI Server] ⚠ 8760-8799 端口均被占用");
            return 0;
        }
    };

    eprintln!("[AI Server] ✅ http://127.0.0.1:{}/v1  (UI: /ui/)", port);

    let _ = std::thread::Builder::new()
        .name("ai-server".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || server_loop(server, predictor, history, ui_dir));

    port
}

// ============================================================
// 服务主循环
// ============================================================

fn send_json(req: tiny_http::Request, status: u16, body: String) {
    let resp = tiny_http::Response::from_string(body)
        .with_status_code(status)
        .with_header(tiny_http::Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap())
        .with_header(tiny_http::Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap());
    let _ = req.respond(resp);
}

fn send_404(req: tiny_http::Request) {
    send_json(req, 404, r#"{"error":{"message":"Not found","type":"error"}}"#.into());
}

fn send_400(req: tiny_http::Request, msg: &str) {
    send_json(req, 400, format!(r#"{{"error":{{"message":"{}","type":"error"}}}}"#, msg));
}

fn server_loop(
    server: tiny_http::Server,
    predictor: Arc<Mutex<AIPredictor>>,
    history: Arc<Mutex<HistoryBuffer>>,
    ui_dir: Option<std::path::PathBuf>,
) {
    const MODEL: &str = "gpt2-chinese-int8";

    for req in server.incoming_requests() {
        let method = req.method().as_str().to_string();
        let url = req.url().to_string();
        let path = url.split('?').next().unwrap_or(&url).to_string();

        // ── GET /ui/* → 静态文件 ─────────────────────────────────
        if method == "GET" && path.starts_with("/ui/") {
            let rel = path.trim_start_matches("/ui/").to_string();
            let content = ui_dir.as_ref()
                .map(|d| d.join(&rel))
                .and_then(|p| std::fs::read(&p).ok());
            match content {
                Some(bytes) => {
                    let mime = mime_type(&rel).to_string();
                    let resp = tiny_http::Response::from_data(bytes)
                        .with_status_code(200)
                        .with_header(tiny_http::Header::from_bytes("Content-Type", mime.as_bytes()).unwrap())
                        .with_header(tiny_http::Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap());
                    let _ = req.respond(resp);
                }
                None => send_404(req),
            }
            continue;
        }

        // ── GET /v1/status ───────────────────────────────────────
        if method == "GET" && (path.starts_with("/v1/status") || path == "/status") {
            let avail = predictor.lock().map(|p| p.is_available()).unwrap_or(false);
            send_json(req, 200, format!(r#"{{"model":"{}","available":{}}}"#, MODEL, avail));
            continue;
        }

        // ── GET /v1/models ───────────────────────────────────────
        if method == "GET" && path.starts_with("/v1/models") {
            send_json(req, 200, format!(r#"{{"object":"list","data":[{{"id":"{}","object":"model"}}]}}"#, MODEL));
            continue;
        }

        // ── OPTIONS ──────────────────────────────────────────────
        if method == "OPTIONS" {
            let resp = tiny_http::Response::from_string("")
                .with_status_code(204)
                .with_header(tiny_http::Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap())
                .with_header(tiny_http::Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap())
                .with_header(tiny_http::Header::from_bytes("Access-Control-Allow-Headers", "Content-Type, Authorization").unwrap());
            let _ = req.respond(resp);
            continue;
        }

        // ── POST /v1/chat/completions ─────────────────────────────
        if method == "POST" && path.starts_with("/v1/chat/completions") {
            // 读取请求体
            let mut body_bytes = Vec::new();
            let mut req = req; // shadow to get mut
            if req.as_reader().read_to_end(&mut body_bytes).is_err() {
                send_400(req, "Failed to read request body");
                continue;
            }
            let chat_req: ChatRequest = match serde_json::from_slice(&body_bytes) {
                Ok(r) => r,
                Err(e) => { send_400(req, &format!("JSON error: {}", e)); continue; }
            };

            let user_content = chat_req.messages.iter().rev()
                .find(|m| m.role == "user")
                .map(|m| m.content.clone())
                .unwrap_or_default();
            let (pinyin, context, dict_words, top_k) = parse_user_message(&user_content);
            let top_k = if top_k == 0 { chat_req.max_tokens.min(9) } else { top_k };

            // 推理
            let candidates: Vec<String> = {
                let ctx_str = if context.is_empty() {
                    history.lock().map(|h| h.context_string()).unwrap_or_default()
                } else {
                    context
                };
                if let Ok(mut pred) = predictor.lock() {
                    if pred.is_available() {
                        pred.predict(&pinyin, &ctx_str, top_k, &dict_words)
                    } else {
                        dict_words.into_iter().take(top_k).collect()
                    }
                } else {
                    vec![]
                }
            };

            let content = candidates.join("\n");
            let resp_obj = ChatResponse {
                id: format!("chatcmpl-{}", timestamp_ms()),
                object: "chat.completion",
                model: MODEL,
                choices: vec![Choice {
                    index: 0,
                    message: ChatMessage { role: "assistant".into(), content },
                    finish_reason: "stop",
                }],
            };
            send_json(req, 200, serde_json::to_string(&resp_obj).unwrap_or_default());
            continue;
        }

        send_404(req);
    }
}

// ============================================================
// 解析 user message
// ============================================================

/// 格式: "拼音：nihao，上文：我今天，候选：你好|拟好|逆号，需要5个"
fn parse_user_message(msg: &str) -> (String, String, Vec<String>, usize) {
    let mut pinyin = String::new();
    let mut context = String::new();
    let mut dict_words = Vec::new();
    let mut top_k = 0usize;

    for part in msg.split(&['，', ','][..]) {
        let part = part.trim();
        if let Some(v) = try_strip(part, &["拼音：", "拼音:"]) {
            pinyin = v.to_string();
        } else if let Some(v) = try_strip(part, &["上文：", "上文:", "上下文：", "上下文:"]) {
            context = v.to_string();
        } else if let Some(v) = try_strip(part, &["候选：", "候选:"]) {
            dict_words = v.split('|').map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()).collect();
        } else if part.contains("需要") {
            let digits: String = part.chars().filter(|c| c.is_ascii_digit()).collect();
            top_k = digits.parse().unwrap_or(0);
        }
    }
    (pinyin, context, dict_words, top_k)
}

fn try_strip<'a>(s: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes.iter().find_map(|p| s.strip_prefix(p))
}

// ============================================================
// 解析外部 LLM 响应 → 有序候选词列表
// ============================================================

/// 解析 chat/completions 响应 content，提取候选词（按返回顺序）
pub fn parse_completion_content(content: &str) -> Vec<String> {
    content.lines()
        .filter_map(|line| {
            let line = strip_list_prefix(line.trim());
            if line.is_empty() { return None; }
            // 若带分数（词语:数字），去掉分数部分
            if let Some(pos) = line.rfind(|c| c == ':' || c == '：') {
                let maybe_score = line[pos+1..].trim();
                if maybe_score.parse::<f32>().is_ok() {
                    let word = line[..pos].trim().to_string();
                    if !word.is_empty() { return Some(word); }
                }
            }
            Some(line.to_string())
        })
        .collect()
}

fn strip_list_prefix(s: &str) -> &str {
    let s = s.trim_start_matches(|c: char| c.is_ascii_digit());
    let s = s.trim_start_matches(['.', '、', '）', ')']);
    let s = s.trim_start_matches(['-', '*', '·']);
    s.trim_start()
}

// ============================================================
// 工具函数
// ============================================================

fn timestamp_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn mime_type(path: &str) -> &'static str {
    if path.ends_with(".html") || path.ends_with(".htm") { "text/html; charset=utf-8" }
    else if path.ends_with(".css") { "text/css; charset=utf-8" }
    else if path.ends_with(".js") { "application/javascript; charset=utf-8" }
    else if path.ends_with(".json") { "application/json" }
    else if path.ends_with(".png") { "image/png" }
    else if path.ends_with(".svg") { "image/svg+xml" }
    else if path.ends_with(".woff2") { "font/woff2" }
    else { "application/octet-stream" }
}


