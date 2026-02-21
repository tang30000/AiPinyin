//! # Guardian 模块 - ctfmon.exe 守护进程
//!
//! 监控 Windows 输入法服务进程 `ctfmon.exe`，
//! 当检测到进程消失时自动重启，确保输入法服务永远在线。
//!
//! ## 设计理念
//! Win11 偶发性的输入法消失 Bug 是很多用户的痛点。
//! Guardian 以后台线程运行，周期性巡检，发现异常立即自愈。

use std::process::Command;
use std::thread;
use std::time::Duration;
use log::{info, warn, error};

/// 守护进程配置
pub struct GuardianConfig {
    /// 巡检间隔（秒）
    pub check_interval_secs: u64,
    /// 最大连续重启次数（防止无限重启风暴）
    pub max_consecutive_restarts: u32,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 5,
            max_consecutive_restarts: 3,
        }
    }
}

/// 检查 ctfmon.exe 是否正在运行
///
/// 通过调用 `tasklist` 命令并过滤进程名来判断。
/// 返回 `true` 表示进程存活，`false` 表示进程消失。
fn is_ctfmon_running() -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq ctfmon.exe", "/FO", "CSV", "/NH"])
        .output();

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            // tasklist 找到进程时输出包含 "ctfmon.exe"
            // 找不到时输出 "INFO: No tasks are running..."
            stdout.to_lowercase().contains("ctfmon.exe")
        }
        Err(e) => {
            error!("[Guardian] 执行 tasklist 失败: {}", e);
            // 无法确认状态时保守处理，假设在运行
            true
        }
    }
}

/// 尝试重启 ctfmon.exe
///
/// 使用 `cmd /c start` 启动进程，避免阻塞当前线程。
fn restart_ctfmon() -> bool {
    info!("[Guardian] 正在重启 ctfmon.exe ...");

    let result = Command::new("cmd")
        .args(["/C", "start", "", "ctfmon.exe"])
        .spawn();

    match result {
        Ok(_) => {
            // 等一小段时间让进程启动
            thread::sleep(Duration::from_millis(500));

            if is_ctfmon_running() {
                info!("[Guardian] ✅ ctfmon.exe 重启成功！");
                true
            } else {
                warn!("[Guardian] ⚠️ ctfmon.exe 重启后未检测到进程");
                false
            }
        }
        Err(e) => {
            error!("[Guardian] ❌ 启动 ctfmon.exe 失败: {}", e);
            false
        }
    }
}

/// 启动守护线程
///
/// 在后台持续监控 ctfmon.exe，发现消失时自动重启。
/// 连续重启失败超过阈值后暂停巡检，避免重启风暴。
///
/// # 示例
/// ```no_run
/// use aipinyin::guardian::{start_guardian, GuardianConfig};
///
/// // 使用默认配置启动守护线程
/// let handle = start_guardian(GuardianConfig::default());
/// ```
pub fn start_guardian(config: GuardianConfig) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        info!(
            "[Guardian] 守护线程已启动 | 巡检间隔: {}s | 最大连续重启: {}次",
            config.check_interval_secs, config.max_consecutive_restarts
        );

        let mut consecutive_failures: u32 = 0;
        let check_interval = Duration::from_secs(config.check_interval_secs);
        // 重启风暴冷却时间: 60秒
        let cooldown = Duration::from_secs(60);

        loop {
            thread::sleep(check_interval);

            if is_ctfmon_running() {
                // 进程正常，重置失败计数
                if consecutive_failures > 0 {
                    info!("[Guardian] ctfmon.exe 已恢复正常运行");
                    consecutive_failures = 0;
                }
            } else {
                warn!("[Guardian] ⚠️ 检测到 ctfmon.exe 已消失！");

                if consecutive_failures >= config.max_consecutive_restarts {
                    error!(
                        "[Guardian] 连续重启失败 {} 次，进入冷却期 {}s",
                        consecutive_failures, cooldown.as_secs()
                    );
                    thread::sleep(cooldown);
                    consecutive_failures = 0;
                    continue;
                }

                if restart_ctfmon() {
                    consecutive_failures = 0;
                } else {
                    consecutive_failures += 1;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ctfmon_detection() {
        // 在 Windows 环境下 ctfmon.exe 通常是运行的
        let running = is_ctfmon_running();
        println!("ctfmon.exe 运行状态: {}", running);
        // 不做硬断言，因为 CI 环境可能没有此进程
    }

    #[test]
    fn test_default_config() {
        let config = GuardianConfig::default();
        assert_eq!(config.check_interval_secs, 5);
        assert_eq!(config.max_consecutive_restarts, 3);
    }
}
