use crate::{
    cmd::{CmdResult, StringifyErr as _},
    utils::dirs::{self, PathBufExec as _},
};
use clash_verge_logging::{Type, logging};
use smartstring::alias::String;
use std::{
    path::{Component, Path, PathBuf},
    time::Duration,
};
use tokio::fs;
use tokio::io::AsyncWriteExt as _;

#[derive(Debug, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct IconInfo {
    #[specta(type = std::string::String)]
    name: String,
    #[specta(type = std::string::String)]
    previous_t: String,
    #[specta(type = std::string::String)]
    current_t: String,
}

fn normalize_icon_segment(name: &str) -> CmdResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err("invalid icon cache file name".into());
    }

    let mut components = Path::new(trimmed).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(trimmed.into()),
        _ => Err("invalid icon cache file name".into()),
    }
}

fn ensure_icon_cache_target(icon_cache_dir: &Path, file_name: &str) -> CmdResult<PathBuf> {
    let icon_path = icon_cache_dir.join(file_name);
    let is_direct_child =
        icon_path.parent().is_some_and(|parent| parent == icon_cache_dir) && icon_path.starts_with(icon_cache_dir);

    if !is_direct_child {
        return Err("invalid icon cache file name".into());
    }

    Ok(icon_path)
}

fn normalized_text_prefix(content: &[u8]) -> std::string::String {
    let content = content.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(content);
    let start = content
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(content.len());
    let end = content.len().min(start.saturating_add(2048));
    let prefix = &content[start..end];
    std::string::String::from_utf8_lossy(prefix).to_ascii_lowercase()
}

fn looks_like_html(content: &[u8]) -> bool {
    let prefix = normalized_text_prefix(content);
    prefix.starts_with("<!doctype html") || prefix.starts_with("<html") || prefix.starts_with("<head")
}

fn looks_like_svg(content: &[u8]) -> bool {
    let prefix = normalized_text_prefix(content);
    prefix.starts_with("<svg")
        || ((prefix.starts_with("<?xml") || prefix.starts_with("<!doctype svg")) && prefix.contains("<svg"))
}

fn is_supported_icon_content(content: &[u8]) -> bool {
    if looks_like_html(content) {
        return false;
    }

    tauri::image::Image::from_bytes(content).is_ok() || looks_like_svg(content)
}

/// Maximum icon download size: a malicious icon host must not be able to
/// exhaust memory by returning an unbounded body.
const MAX_ICON_BYTES: usize = 5 * 1024 * 1024;
/// Total time budget for an icon download.
const ICON_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn download_icon_cache(url: String, name: String) -> CmdResult<String> {
    let icon_cache_dir = dirs::app_home_dir().stringify_err()?.join("icons").join("cache");
    let icon_name = normalize_icon_segment(name.as_str())?;
    let icon_path = ensure_icon_cache_target(&icon_cache_dir, icon_name.as_str())?;

    if icon_path.exists() {
        return Ok(icon_path.to_string_lossy().into());
    }

    if !icon_cache_dir.exists() {
        fs::create_dir_all(&icon_cache_dir).await.stringify_err()?;
    }

    let temp_name = format!("{icon_name}.downloading");
    let temp_path = ensure_icon_cache_target(&icon_cache_dir, temp_name.as_str())?;

    // Icons arrive from subscription-authored group fields and user-typed
    // test items, so the URL is untrusted: http(s) only, with a host.
    // (No allowlist: the fetch receives only (url, name), and no field in
    // the codebase holds an icon-host allowlist to key one on.)
    let parsed = tauri::Url::parse(url.as_str()).map_err(|_| "invalid icon URL")?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("icon URL must use http or https".into());
    }

    let client = reqwest::Client::builder()
        .timeout(ICON_DOWNLOAD_TIMEOUT)
        .build()
        .stringify_err()?;
    let mut response = client.get(parsed).send().await.stringify_err()?;
    response = response.error_for_status().stringify_err()?;
    if response.content_length().is_some_and(|len| len > MAX_ICON_BYTES as u64) {
        return Err(format!("icon exceeds the {MAX_ICON_BYTES} byte limit").into());
    }
    let mut content = Vec::new();
    while let Some(chunk) = response.chunk().await.stringify_err()? {
        content.extend_from_slice(&chunk);
        if content.len() > MAX_ICON_BYTES {
            return Err(format!("icon exceeds the {MAX_ICON_BYTES} byte limit").into());
        }
    }

    if !is_supported_icon_content(&content) {
        let _ = temp_path.remove_if_exists().await;
        return Err(format!("Downloaded content is not a valid image: {}", url.as_str()).into());
    }

    {
        let mut file = match fs::File::create(&temp_path).await {
            Ok(file) => file,
            Err(_) => {
                if icon_path.exists() {
                    return Ok(icon_path.to_string_lossy().into());
                }
                return Err("Failed to create temporary file".into());
            }
        };
        file.write_all(content.as_ref()).await.stringify_err()?;
        file.flush().await.stringify_err()?;
    }

    if !icon_path.exists() {
        match fs::rename(&temp_path, &icon_path).await {
            Ok(_) => {}
            Err(_) => {
                let _ = temp_path.remove_if_exists().await;
                if icon_path.exists() {
                    return Ok(icon_path.to_string_lossy().into());
                }
            }
        }
    } else {
        let _ = temp_path.remove_if_exists().await;
    }

    Ok(icon_path.to_string_lossy().into())
}

pub async fn copy_icon_file(path: String, icon_info: IconInfo) -> CmdResult<String> {
    let file_path = Path::new(path.as_str());
    let icon_name = normalize_icon_segment(icon_info.name.as_str())?;
    let current_t = normalize_icon_segment(icon_info.current_t.as_str())?;
    let previous_t = if icon_info.previous_t.trim().is_empty() {
        None
    } else {
        Some(normalize_icon_segment(icon_info.previous_t.as_str())?)
    };

    let icon_dir = dirs::app_home_dir().stringify_err()?.join("icons");
    if !icon_dir.exists() {
        fs::create_dir_all(&icon_dir).await.stringify_err()?;
    }

    let ext: String = match file_path.extension() {
        Some(e) => e.to_string_lossy().into(),
        None => "ico".into(),
    };

    let dest_file_name = format!("{icon_name}-{current_t}.{ext}");
    let dest_path = ensure_icon_cache_target(&icon_dir, dest_file_name.as_str())?;

    if file_path.exists() {
        if let Some(previous_t) = previous_t {
            let previous_png = ensure_icon_cache_target(&icon_dir, format!("{icon_name}-{previous_t}.png").as_str())?;
            previous_png.remove_if_exists().await.unwrap_or_default();
            let previous_ico = ensure_icon_cache_target(&icon_dir, format!("{icon_name}-{previous_t}.ico").as_str())?;
            previous_ico.remove_if_exists().await.unwrap_or_default();
        }

        logging!(debug, Type::Cmd, "copying icon file: {path} -> {}", dest_path.display());

        match fs::copy(file_path, &dest_path).await {
            Ok(_) => Ok(dest_path.to_string_lossy().into()),
            Err(err) => Err(err.to_string().into()),
        }
    } else {
        Err("file not found".into())
    }
}
