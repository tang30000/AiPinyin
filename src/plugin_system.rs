//! # 插件系统 — JS 沙箱 + 授权 + 槽位管理
//!
//! ## 设计
//! - 每个 .js 文件在独立 Context（沙箱隔离）中运行
//! - 最多同时启用 5 个插件（MAX_ACTIVE）
//! - 首次启用时需用户授权（持久化到 plugins/.authorized）
//! - 提供 `on_candidates(raw, candidates)` 钩子

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use rquickjs::{Context, Ctx, Function, Object, Runtime, Value};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

// ── 常量 ──────────────────────────────────────────────────────
pub const MAX_ACTIVE: usize = 5;
const AUTH_FILE: &str = ".authorized";

// ============================================================
// 公开类型
// ============================================================

/// 插件的当前状态快照（用于 UI 展示）
pub struct PluginInfo {
    pub name: String,
    pub enabled: bool,
    pub authorized: bool,
}

/// toggle() 操作的结果
pub enum ToggleResult {
    Enabled,
    Disabled,
    SlotsFull,  // 已达 MAX_ACTIVE 限制
    Denied,     // 用户拒绝授权
}

// ============================================================
// PluginSystem
// ============================================================

pub struct PluginSystem {
    _runtime: Runtime,
    plugins: Vec<LoadedPlugin>,
    /// 已授权的插件名称集合（持久化）
    authorized: HashSet<String>,
    plugins_dir: PathBuf,
}

struct LoadedPlugin {
    name: String,
    ctx: Context,
    enabled: bool,
}

impl PluginSystem {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            _runtime: Runtime::new()?,
            plugins: Vec::new(),
            authorized: HashSet::new(),
            plugins_dir: PathBuf::new(),
        })
    }

    /// 扫描并加载目录中的所有 .js 文件
    pub fn load_dir(&mut self, dir: &Path) {
        self.plugins_dir = dir.to_path_buf();
        self.authorized = Self::read_authorized(dir);

        if !dir.exists() { return; }

        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("js"))
                    .collect()
            })
            .unwrap_or_default();
        paths.sort();

        for path in &paths {
            match self.load_file(path) {
                Ok(()) => {}
                Err(e) => eprintln!(
                    "[Plugin] ❌ {:?}: {}",
                    path.file_name().unwrap_or_default(), e
                ),
            }
        }

        if !self.plugins.is_empty() {
            eprintln!("[Plugin] 已加载 {} 个插件 (授权 {} 个, 激活 {} 个)",
                self.plugins.len(), self.authorized.len(), self.active_count());
        }
    }

    fn load_file(&mut self, path: &Path) -> anyhow::Result<()> {
        let name = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("plugin")
            .to_string();

        let code = std::fs::read_to_string(path)?;
        let ctx = Context::full(&self._runtime)?;
        let pname = name.clone();

        ctx.with(|ctx| -> rquickjs::Result<()> {
            inject_globals(ctx.clone(), &pname)?;
            ctx.eval::<(), _>(code.as_bytes())?;
            Ok(())
        })?;

        // 已授权的插件默认启用
        let enabled = self.authorized.contains(&name);
        eprintln!("[Plugin] ✅ {}.js  ({})", name,
            if enabled { "已启用" } else { "待授权/已禁用" });

        self.plugins.push(LoadedPlugin { name, ctx, enabled });
        Ok(())
    }

    // ── 公开查询 API ──────────────────────────────────────────

    pub fn plugin_list(&self) -> Vec<PluginInfo> {
        self.plugins.iter().map(|p| PluginInfo {
            name: p.name.clone(),
            enabled: p.enabled,
            authorized: self.authorized.contains(&p.name),
        }).collect()
    }

    pub fn active_count(&self) -> usize {
        self.plugins.iter().filter(|p| p.enabled).count()
    }

    pub fn has_active(&self) -> bool { self.active_count() > 0 }
    pub fn is_loaded(&self) -> bool { !self.plugins.is_empty() }

    // ── 启用/禁用切换 ─────────────────────────────────────────

    /// 切换插件启用状态
    ///
    /// - 禁用时：直接禁用，无需确认
    /// - 首次启用时：弹出授权对话框，用户同意后才启用
    /// - 已达 MAX_ACTIVE 时：弹出槽位已满提示
    pub fn toggle(&mut self, name: &str, parent: HWND) -> ToggleResult {
        let idx = match self.plugins.iter().position(|p| p.name == name) {
            Some(i) => i,
            None => return ToggleResult::Denied,
        };

        if self.plugins[idx].enabled {
            // 禁用：直接关掉
            self.plugins[idx].enabled = false;
            eprintln!("[Plugin] ⏸ {} 已禁用", name);
            return ToggleResult::Disabled;
        }

        // 启用前：检查授权
        if !self.authorized.contains(name) {
            let msg = format!(
                "插件「{}」将访问您的输入流，读取并可能修改每次输入的候\
选词。\n\n是否授权该插件？", name
            );
            let msg_w: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
            let caption_w: Vec<u16> = "AiPinyin 插件授权"
                .encode_utf16().chain(std::iter::once(0)).collect();

            let result = unsafe {
                MessageBoxW(
                    parent,
                    PCWSTR(msg_w.as_ptr()),
                    PCWSTR(caption_w.as_ptr()),
                    MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
                )
            };

            if result != IDYES {
                eprintln!("[Plugin] 🚫 用户拒绝授权 {}", name);
                return ToggleResult::Denied;
            }

            self.authorized.insert(name.to_string());
            self.write_authorized();
            eprintln!("[Plugin] 🔑 {} 已授权并持久化", name);
        }

        // 检查槽位
        if self.active_count() >= MAX_ACTIVE {
            let msg_w: Vec<u16> = format!(
                "插件槽位已满（最多 {} 个同时激活）。\n请先禁用一个插件再启用新插件。",
                MAX_ACTIVE
            ).encode_utf16().chain(std::iter::once(0)).collect();
            let cap_w: Vec<u16> = "AiPinyin 插件管理"
                .encode_utf16().chain(std::iter::once(0)).collect();

            unsafe {
                MessageBoxW(parent,
                    PCWSTR(msg_w.as_ptr()), PCWSTR(cap_w.as_ptr()),
                    MB_OK | MB_ICONINFORMATION);
            }
            return ToggleResult::SlotsFull;
        }

        self.plugins[idx].enabled = true;
        eprintln!("[Plugin] ▶ {} 已启用 ({}/{}活跃)",
            name, self.active_count(), MAX_ACTIVE);
        ToggleResult::Enabled
    }

    // ── 候选词处理 ────────────────────────────────────────────

    /// 依次通过所有已启用的插件处理候选词（流水线）
    pub fn transform_candidates(&self, raw: &str, mut cands: Vec<String>) -> Vec<String> {
        for p in self.plugins.iter().filter(|p| p.enabled) {
            cands = p.call_on_candidates(raw, cands);
        }
        cands
    }

    // ── 授权持久化 ────────────────────────────────────────────

    fn read_authorized(dir: &Path) -> HashSet<String> {
        std::fs::read_to_string(dir.join(AUTH_FILE))
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect()
    }

    fn write_authorized(&self) {
        let mut lines: Vec<&str> = self.authorized.iter().map(|s| s.as_str()).collect();
        lines.sort();
        let content = format!("# AiPinyin 已授权插件列表（自动生成）\n{}\n", lines.join("\n"));
        let _ = std::fs::write(self.plugins_dir.join(AUTH_FILE), content);
    }
}

// ============================================================
// LoadedPlugin — JS 执行
// ============================================================

impl LoadedPlugin {
    fn call_on_candidates(&self, raw: &str, candidates: Vec<String>) -> Vec<String> {
        let fallback = candidates.clone();
        let raw_owned = raw.to_string();

        let result = self.ctx.with(|ctx| -> rquickjs::Result<Vec<String>> {
            let globals = ctx.globals();
            let val: Value = globals.get("on_candidates")?;
            if !val.is_function() { return Ok(candidates); }
            let func = Function::from_value(val)?;

            let js_arr = rquickjs::Array::new(ctx.clone())?;
            for (i, c) in candidates.iter().enumerate() {
                js_arr.set(i, c.as_str())?;
            }

            let ret: Value = func.call((raw_owned.as_str(), js_arr))?;

            if !ret.is_array() { return Ok(candidates); }
            let arr = rquickjs::Array::from_value(ret)?;
            let mut out: Vec<String> = Vec::new();
            for i in 0..arr.len() {
                if let Ok(s) = arr.get::<String>(i) { out.push(s); }
            }
            if out.is_empty() { Ok(candidates) } else { Ok(out) }
        });

        result.unwrap_or(fallback)
    }
}

// ============================================================
// inject_globals — 向沙箱注入宿主 API
// ============================================================

fn inject_globals(ctx: Ctx<'_>, plugin_name: &str) -> rquickjs::Result<()> {
    let console = Object::new(ctx.clone())?;

    let n = plugin_name.to_string();
    console.set("log", Function::new(ctx.clone(), move |msg: rquickjs::Coerced<String>| {
        println!("[{}] {}", n, msg.0);
    })?)?;

    let n = plugin_name.to_string();
    console.set("warn", Function::new(ctx.clone(), move |msg: rquickjs::Coerced<String>| {
        eprintln!("[{}] ⚠ {}", n, msg.0);
    })?)?;

    let n = plugin_name.to_string();
    console.set("error", Function::new(ctx.clone(), move |msg: rquickjs::Coerced<String>| {
        eprintln!("[{}] ✖ {}", n, msg.0);
    })?)?;

    ctx.globals().set("console", console)?;
    Ok(())
}
