//! 系统交互命令:用文件管理器/默认程序打开成果文件或所在文件夹

use std::process::Command;

/// 后台收割子进程,避免僵尸进程
fn spawn_detached(mut cmd: Command) -> Result<(), String> {
    let mut child = cmd.spawn().map_err(|e| format!("启动失败: {}", e))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// 用系统默认程序打开文件或文件夹
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", ""]).arg(&path);
        spawn_detached(cmd)
    }
    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("open");
        cmd.arg(&path);
        spawn_detached(cmd)
    }
    #[cfg(target_os = "linux")]
    {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(&path);
        spawn_detached(cmd)
    }
}

/// 打开文件所在文件夹并高亮该文件(Linux 退化为打开父目录)
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // explorer /select, 要求反斜杠路径
        let win_path = path.replace('/', "\\");
        let mut cmd = Command::new("explorer");
        cmd.arg(format!("/select,{}", win_path));
        // explorer 的退出码非零也算成功(常见怪癖),只关心能否启动
        match cmd.spawn() {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("启动失败: {}", e)),
        }
    }
    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("open");
        cmd.arg("-R").arg(&path);
        spawn_detached(cmd)
    }
    #[cfg(target_os = "linux")]
    {
        // xdg-open 无法高亮单个文件,退化为打开父目录
        let parent = std::path::Path::new(&path)
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| format!("无法获取父目录: {}", path))?;
        let mut cmd = Command::new("xdg-open");
        cmd.arg(&parent);
        spawn_detached(cmd)
    }
}
