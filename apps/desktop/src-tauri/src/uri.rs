//! Android 的文件选择器交出的是 `content://` URI（SAF 授权），而导入管线按
//! 路径工作。这里把 URI 的字节读出来落到应用缓存，返回一个真实路径；其它平台
//! 原样返回传入的路径。

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ImportSource {
    /// 导入管线实际打开的真实路径。
    pub path: PathBuf,
    /// 一份副本要被清掉时才记下来（当前总是应用缓存，交给系统回收）。
    pub temporary: bool,
}

/// 一次导入允许的最大字节数：与阅读库的正文规模保持一致，防止把大文件复制
/// 进缓存后才发现无意义。
pub const MAX_IMPORT_BYTES: u64 = 64 * 1024 * 1024;

#[cfg(not(target_os = "android"))]
pub fn materialize(path: &str) -> Result<ImportSource, String> {
    Ok(ImportSource {
        path: PathBuf::from(path),
        temporary: false,
    })
}

#[cfg(target_os = "android")]
pub fn materialize(path: &str) -> Result<ImportSource, String> {
    if !path.starts_with("content://") {
        return Ok(ImportSource {
            path: PathBuf::from(path),
            temporary: false,
        });
    }
    let (name, bytes) = bridge::read(path)?;
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err(format!(
            "文件太大（{} MB），上限 {} MB",
            bytes.len() / (1024 * 1024),
            MAX_IMPORT_BYTES / (1024 * 1024)
        ));
    }
    let cache = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_dir())
        .or_else(|| crate::mobile_data_dir())
        .unwrap_or_else(std::env::temp_dir);
    let file_name = sanitize_name(&name);
    let target = cache.join(format!("import-{}-{file_name}", std::process::id()));
    std::fs::write(&target, &bytes).map_err(|error| format!("写入导入缓存失败：{error}"))?;
    Ok(ImportSource {
        path: target,
        temporary: true,
    })
}

#[cfg(target_os = "android")]
fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    if cleaned.is_empty() || !cleaned.contains('.') {
        "import.html".to_string()
    } else {
        cleaned
    }
}

#[cfg(target_os = "android")]
mod bridge {
    use base64::Engine as _;
    use tauri::plugin::PluginHandle;

    static URI: std::sync::OnceLock<PluginHandle<tauri::Wry>> = std::sync::OnceLock::new();

    pub fn install(handle: PluginHandle<tauri::Wry>) {
        let _ = URI.set(handle);
    }

    #[derive(serde::Deserialize)]
    struct Contents {
        data: String,
        #[allow(dead_code)]
        name: String,
    }

    pub fn read(uri: &str) -> Result<(String, Vec<u8>), String> {
        let handle = URI
            .get()
            .ok_or_else(|| "SAF 读取插件未初始化".to_string())?;
        let contents: Contents = handle
            .run_mobile_plugin("read", serde_json::json!({ "uri": uri }))
            .map_err(|error| format!("读取所选文件失败：{error}"))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(contents.data.as_bytes())
            .map_err(|error| format!("所选文件内容损坏：{error}"))?;
        Ok((contents.name, bytes))
    }
}

#[cfg(target_os = "android")]
pub use bridge::install;
