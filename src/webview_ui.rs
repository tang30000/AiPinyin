//! # WebView UI 模块 — 统一输入条与设置界面
//!
//! 使用 wry + tao 创建全局常驻的透明 WebView2 窗口。

use anyhow::Result;
use std::path::PathBuf;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::windows::{EventLoopBuilderExtWindows, WindowExtWindows};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use serde::Serialize;

// JSON IPC structures
#[derive(Serialize)]
struct ImeUpdateMsg<'a> {
    #[serde(rename = "type")]
    msg_type: &'static str,
    raw: String,
    candidates: &'a [String],
    page: usize,
    total_pages: usize,
}

#[derive(Serialize)]
struct HideMsg {
    #[serde(rename = "type")]
    msg_type: &'static str,
}

#[derive(Serialize)]
struct ShowSettingsMsg {
    #[serde(rename = "type")]
    msg_type: &'static str,
}

#[derive(Serialize)]
struct PluginsActiveMsg {
    #[serde(rename = "type")]
    msg_type: &'static str,
    active: bool,
}

pub enum ImeEvent {
    ShowAt(i32, i32),
    Hide,
    UpdateCandidates { raw: String, candidates: Vec<String>, page_info: Option<(usize, usize)> },
    ShowSettings,
    PluginsActive(bool),
    LayoutUpdate { width: f64, height: f64 },
    DragWindow { dx: f64, dy: f64 },
}

pub struct WebViewUI {
    proxy: EventLoopProxy<ImeEvent>,
    hwnd: HWND,
}

impl WebViewUI {
    pub fn new() -> Result<(Self, tao::event_loop::EventLoop<ImeEvent>)> {
        let event_loop = EventLoopBuilder::<ImeEvent>::with_user_event()
            .with_any_thread(true)
            .build();
        
        let proxy = event_loop.create_proxy();
        let hwnd = HWND(std::ptr::null_mut()); // This will be assigned after creation or we refactor window creation out. For now, we will return a dummy and update it later. Actually, wait. The Window is created INSIDE run_webview_loop which is spawned AFTER WebViewUI!
        // To fix this, we need to create the Window BEFORE run_webview_loop, or just not use `hwnd()` from WebViewUI in main.rs. Let's look at main.rs usage.
        
        Ok((Self { proxy, hwnd }, event_loop))
    }

    pub fn set_hwnd(&mut self, hwnd: HWND) {
        self.hwnd = hwnd;
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn draw_candidates(&self, candidates: &[&str]) {
        let _ = self.proxy.send_event(ImeEvent::UpdateCandidates {
            raw: String::new(),
            candidates: candidates.iter().map(|s| s.to_string()).collect(),
            page_info: None,
        });
    }

    pub fn set_raw_input(&self, raw: &str) {
        let _ = self.proxy.send_event(ImeEvent::UpdateCandidates {
            raw: raw.to_string(),
            candidates: vec![],
            page_info: None,
        });
    }

    pub fn update_candidates_with_page(&self, raw: &str, candidates: &[&str], page_info: Option<(usize, usize)>) {
        let _ = self.proxy.send_event(ImeEvent::UpdateCandidates {
            raw: raw.to_string(),
            candidates: candidates.iter().map(|s| s.to_string()).collect(),
            page_info,
        });
    }

    pub fn set_plugins_active(&self, active: bool) {
        let _ = self.proxy.send_event(ImeEvent::PluginsActive(active));
    }

    pub fn hide(&self) {
        let _ = self.proxy.send_event(ImeEvent::Hide);
    }

    pub fn show(&self, x: i32, y: i32) {
        let _ = self.proxy.send_event(ImeEvent::ShowAt(x, y));
    }

    pub fn open_settings(&self) {
        let _ = self.proxy.send_event(ImeEvent::ShowSettings);
    }
}

pub fn run_webview_loop(
    event_loop: tao::event_loop::EventLoop<ImeEvent>,
    ai_port: u16,
) -> Result<()> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = exe_dir; // 保留备用

    let window = WindowBuilder::new()
        .with_title("AiPinyin")
        .with_inner_size(tao::dpi::LogicalSize::new(300.0, 50.0))
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top(true)
        .with_visible(false)
        .build(&event_loop)?;

    let hwnd = HWND(window.hwnd() as *mut _);
    unsafe {
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | (WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE).0 as i32);
    }

    // JS 初始化脚本注入配置和 ai_port
    let config_json = crate::settings::load_config_json();
    let init_script = format!(
        "window.__INIT_CONFIG__ = {}; window.__AI_PORT__ = {};",
        config_json, ai_port
    );

    // 确定 UI 加载地址
    // 优先用本地 HTTP 服务（ai_server 已在同一端口提供 /ui/ 文件）
    // 也可在 config.toml 中配置 ui_url 指向主题市场的远程地址
    let ui_url = if ai_port > 0 {
        format!("http://127.0.0.1:{}/ui/index.html", ai_port)
    } else {
        FALLBACK_HTML.to_string() // 服务未启动时用内嵌 fallback
    };

    let builder = WebViewBuilder::new()
        .with_transparent(true)
        .with_background_color((0, 0, 0, 0))
        .with_initialization_script(&init_script);

    let builder = if ai_port > 0 {
        builder.with_url(&ui_url)
    } else {
        builder.with_html(FALLBACK_HTML)
    };


    let proxy = event_loop.create_proxy();
    
    let webview = builder
        .with_ipc_handler(move |msg| {
            let body = msg.body();
            match serde_json::from_str::<serde_json::Value>(body) {
                Ok(data) => {
                    let action = data["action"].as_str().unwrap_or("");
                    match action {
                        "save" => {
                            crate::settings::save_config(&data);
                            crate::settings::save_style(&data);
                        }
                        "toggle_plugin" => {
                            if let Some(name) = data["name"].as_str() {
                                let enabled = data["enabled"].as_bool().unwrap_or(false);
                                crate::settings::toggle_plugin(name, enabled);
                            }
                        }
                        "delete_plugin" => {
                            if let Some(name) = data["name"].as_str() {
                                crate::settings::delete_plugin(name);
                            }
                        }
                        "layout_update" => {
                            if let (Some(w), Some(h)) = (data["width"].as_f64(), data["height"].as_f64()) {
                                let _ = proxy.send_event(ImeEvent::LayoutUpdate { width: w, height: h });
                            }
                        }
                        "drag_window" => {
                            if let (Some(dx), Some(dy)) = (data["dx"].as_f64(), data["dy"].as_f64()) {
                                let _ = proxy.send_event(ImeEvent::DragWindow { dx, dy });
                            }
                        }
                        _ => {}
                    }
                }
                Err(e) => eprintln!("[WebView UI] IPC parse error: {}", e),
            }
        })
        .build(&window)?;

    // Keep it alive
    let _webview_keep = webview;

    // Track current position to enable dragging correctly
    let mut current_x: f64 = 0.0;
    let mut current_y: f64 = 0.0;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(ime_event) => {
                match ime_event {
                    ImeEvent::ShowAt(x, y) => {
                        current_x = x as f64;
                        current_y = y as f64;
                        window.set_outer_position(tao::dpi::LogicalPosition::new(current_x, current_y));
                        window.set_visible(true);
                    }
                    ImeEvent::Hide => {
                        window.set_visible(false);
                        let msg = HideMsg { msg_type: "hide" };
                        if let Ok(json) = serde_json::to_string(&msg) {
                            let _ = _webview_keep.evaluate_script(&format!("window.postMessage({}, '*');", json));
                        }
                    }
                    ImeEvent::UpdateCandidates { raw, candidates, page_info } => {
                        let (page, total_pages) = page_info.unwrap_or((1, 1));
                        let msg = ImeUpdateMsg {
                            msg_type: "show_ime",
                            raw: raw.clone(),
                            candidates: &candidates,
                            page,
                            total_pages,
                        };
                        
                        if let Ok(json) = serde_json::to_string(&msg) {
                            let script = format!("window.postMessage({}, '*');", json);
                            let _ = _webview_keep.evaluate_script(&script);
                            
                            // Rough estimation to expand window so JS flexbox doesn't wrap lines prematurely
                            // before the layout_update message computes the exact bounding box.
                            let est_w = 60.0 + (candidates.len() as f64 * 35.0);
                            window.set_inner_size(tao::dpi::LogicalSize::new(est_w.min(1500.0), 80.0));
                        }
                    }
                    ImeEvent::ShowSettings => {
                        let msg = ShowSettingsMsg { msg_type: "show_settings" };
                        if let Ok(json) = serde_json::to_string(&msg) {
                            let _ = _webview_keep.evaluate_script(&format!("window.postMessage({}, '*');", json));
                        }
                        // Center window and make it larger
                        window.set_inner_size(tao::dpi::LogicalSize::new(520.0, 720.0));
                        
                        unsafe {
                            let cx = GetSystemMetrics(SM_CXSCREEN);
                            let cy = GetSystemMetrics(SM_CYSCREEN);
                            window.set_outer_position(tao::dpi::LogicalPosition::new(
                                (cx as f64 - 520.0) / 2.0,
                                (cy as f64 - 720.0) / 2.0
                            ));
                            
                            let hwnd = HWND(window.hwnd() as *mut _);
                            SetForegroundWindow(hwnd);
                        }
                        window.set_visible(true);
                    }
                    ImeEvent::PluginsActive(active) => {
                        let msg = PluginsActiveMsg { msg_type: "plugins_active", active };
                        if let Ok(json) = serde_json::to_string(&msg) {
                            let _ = _webview_keep.evaluate_script(&format!("window.postMessage({}, '*');", json));
                        }
                    }
                    ImeEvent::LayoutUpdate { width, height } => {
                        // Dynamically snap the tao window tightly to the content size
                        // This entirely removes any "white OS background" spillage since the window matches the UI bounds
                        window.set_inner_size(tao::dpi::LogicalSize::new(width, height));
                        
                        // Detect and prevent right-edge overflow
                        unsafe {
                            let cx = GetSystemMetrics(SM_CXSCREEN) as f64;
                            // If window X + layout_width > screen_width, push it left
                            if current_x + width > cx {
                                current_x = cx - width - 10.0; // 10px buffer
                                window.set_outer_position(tao::dpi::LogicalPosition::new(current_x, current_y));
                            }
                        }
                    }
                    ImeEvent::DragWindow { dx, dy } => {
                        current_x += dx;
                        current_y += dy;
                        window.set_outer_position(tao::dpi::LogicalPosition::new(current_x, current_y));
                    }
                }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                // Ignore close, just hide
                window.set_visible(false);
                let msg = HideMsg { msg_type: "hide" };
                if let Ok(json) = serde_json::to_string(&msg) {
                    let _ = _webview_keep.evaluate_script(&format!("window.postMessage({}, '*');", json));
                }
            }
            _ => {}
        }
    });
}

// ============================================================
// 辅助常量和函数
// ============================================================

const FALLBACK_HTML: &str = r#"<!DOCTYPE html><html><head><meta charset="utf-8">
<style>body{background:transparent;margin:0;overflow:hidden;}
#b{background:rgba(0,0,0,.85);color:#fff;padding:10px;border-radius:8px;
   font-family:sans-serif;display:none;position:fixed;top:4px;left:4px;}
</style></head><body><div id="b"></div><script>
window.addEventListener('message',e=>{
  const d=e.data;
  if(d&&d.type==='show_ime'){document.getElementById('b').style.display='block';
    document.getElementById('b').textContent=d.raw;}
  else if(d&&d.type==='hide'){document.getElementById('b').style.display='none';}
});
</script></body></html>"#;

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
