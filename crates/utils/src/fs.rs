//! 文件系统工具函数

use std::path::Path;
use anyhow::{Context, Result};

/// 读取文件内容为字符串
pub fn read_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))
}

/// 写入字符串到文件
pub fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }
    std::fs::write(path, content)
        .with_context(|| format!("Failed to write file: {}", path.display()))
}

/// 追加内容到文件
pub fn append_file(path: &Path, content: &str) -> Result<()> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, content.as_bytes()))
        .with_context(|| format!("Failed to append to file: {}", path.display()))
}

/// 检查文件是否存在
pub fn exists(path: &Path) -> bool {
    path.exists()
}

/// 列出目录中的所有文件（递归）
pub fn list_files(dir: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Ok(path) = entry.path().strip_prefix(dir) {
                files.push(path.to_string_lossy().to_string());
            }
        }
    }
    Ok(files)
}
