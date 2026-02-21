//! # 插件系统 — JS 沙箱 (rquickjs / QuickJS-NG)
//!
//! 每个 .js 文件在独立 Context 中运行（共享同一 Runtime），彼此隔离。
//!
//! ## 插件 API （在 .js 中定义）
//!
//! ```js
//! // 候选词钩子：返回修改后的候选词数组
//! function on_candidates(raw, candidates) {
//!     // raw: 当前拼音输入（String）
//!     // candidates: 当前候选词数组（Array<String>）
//!     return candidates; // 原样返回 = 不修改
//! }
//! ```
//!
//! ## 内置全局对象
//! - `console.log(msg)` : 打印到控制台
//! - `console.warn(msg)`: 打印警告
//! - `Date`             : 标准 JS Date（内置于 QuickJS）

use std::path::{Path, PathBuf};
use rquickjs::{Context, Ctx, Function, Object, Runtime, Value};

// ============================================================
// PluginSystem
// ============================================================

pub struct PluginSystem {
    /// Runtime 必须比所有 Context 生命周期更长
    _runtime: Runtime,
    plugins: Vec<LoadedPlugin>,
}

struct LoadedPlugin {
    name: String,
    ctx: Context,
}

impl PluginSystem {
    pub fn new() -> anyhow::Result<Self> {
        let runtime = Runtime::new()?;
        Ok(Self { _runtime: runtime, plugins: Vec::new() })
    }

    /// 扫描目录，加载所有 .js 文件（按文件名字母排序，保证顺序可预期）
    pub fn load_dir(&mut self, dir: &Path) {
        if !dir.exists() {
            return; // 没有 plugins/ 目录是正常情况，静默跳过
        }

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
                    path.file_name().unwrap_or_default(),
                    e
                ),
            }
        }

        if !paths.is_empty() {
            eprintln!("[Plugin] 共加载 {} 个插件", self.plugins.len());
        }
    }

    fn load_file(&mut self, path: &Path) -> anyhow::Result<()> {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("plugin")
            .to_string();

        let code = std::fs::read_to_string(path)?;

        // 每个插件独立 Context（沙箱隔离）
        let ctx = Context::full(&self._runtime)?;

        let plugin_name = name.clone();
        ctx.with(|ctx| -> rquickjs::Result<()> {
            inject_globals(ctx.clone(), &plugin_name)?;
            ctx.eval::<(), _>(code.as_bytes())?;
            Ok(())
        })?;

        eprintln!("[Plugin] ✅ {}.js", name);
        self.plugins.push(LoadedPlugin { name, ctx });
        Ok(())
    }

    /// 依次通过所有插件处理候选词（流水线）
    ///
    /// 前一个插件的输出是后一个插件的输入。
    pub fn transform_candidates(&self, raw: &str, mut candidates: Vec<String>) -> Vec<String> {
        for plugin in &self.plugins {
            candidates = plugin.call_on_candidates(raw, candidates);
        }
        candidates
    }

    pub fn is_loaded(&self) -> bool {
        !self.plugins.is_empty()
    }
}

// ============================================================
// LoadedPlugin — 单个插件执行
// ============================================================

impl LoadedPlugin {
    fn call_on_candidates(&self, raw: &str, candidates: Vec<String>) -> Vec<String> {
        let fallback = candidates.clone();
        let raw_owned = raw.to_string();

        let result = self.ctx.with(|ctx| -> rquickjs::Result<Vec<String>> {
            let globals = ctx.globals();

            // 检查 on_candidates 是否存在且是函数
            let val: Value = globals.get("on_candidates")?;
            if !val.is_function() {
                return Ok(candidates);
            }
            let func = Function::from_value(val)?;

            // 构建候选词 JS 数组
            let js_arr = rquickjs::Array::new(ctx.clone())?;
            for (i, c) in candidates.iter().enumerate() {
                js_arr.set(i, c.as_str())?;
            }

            // 调用 on_candidates(raw, [candidates...])
            let ret: Value = func.call((raw_owned.as_str(), js_arr))?;

            // 解析返回值
            if !ret.is_array() {
                // 没有返回有效数组则保持原候选
                return Ok(candidates);
            }
            let arr = rquickjs::Array::from_value(ret)?;
            let mut out: Vec<String> = Vec::new();
            for i in 0..arr.len() {
                if let Ok(s) = arr.get::<String>(i) {
                    out.push(s);
                }
            }

            if out.is_empty() { Ok(candidates) } else { Ok(out) }
        });

        match result {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[Plugin] {} 运行出错: {}", self.name, e);
                fallback
            }
        }
    }
}

// ============================================================
// inject_globals — 向沙箱注入宿主 API
// ============================================================

fn inject_globals(ctx: Ctx<'_>, plugin_name: &str) -> rquickjs::Result<()> {
    let console = Object::new(ctx.clone())?;

    // console.log
    let n = plugin_name.to_string();
    console.set(
        "log",
        Function::new(ctx.clone(), move |msg: rquickjs::Coerced<String>| {
            println!("[{}] {}", n, msg.0);
        })?,
    )?;

    // console.warn
    let n = plugin_name.to_string();
    console.set(
        "warn",
        Function::new(ctx.clone(), move |msg: rquickjs::Coerced<String>| {
            eprintln!("[{}] ⚠ {}", n, msg.0);
        })?,
    )?;

    // console.error
    let n = plugin_name.to_string();
    console.set(
        "error",
        Function::new(ctx.clone(), move |msg: rquickjs::Coerced<String>| {
            eprintln!("[{}] ✖ {}", n, msg.0);
        })?,
    )?;

    ctx.globals().set("console", console)?;
    Ok(())
}
