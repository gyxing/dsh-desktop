use std::{
    ffi::OsStr,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use serde_json::Value;

const MIRAGE_BUNDLE: &str = "@struktoai/mirage-dsh";

/// 按上游优先级解析 DSH_HOME；只返回路径，不创建或修改用户目录。
pub fn resolve_dsh_home(
    home_directory: &Path,
    current_directory: &Path,
    configured: Option<&OsStr>,
) -> PathBuf {
    let configured = configured.filter(|value| !value.to_string_lossy().trim().is_empty());
    let selected = configured
        .map(|value| {
            let text = value.to_string_lossy();
            if text == "~" {
                home_directory.to_path_buf()
            } else if let Some(relative) =
                text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\"))
            {
                home_directory.join(relative)
            } else {
                PathBuf::from(value)
            }
        })
        .unwrap_or_else(|| home_directory.join(".dsh"));

    if selected.is_absolute() {
        selected
    } else {
        current_directory.join(selected)
    }
}

/// 检查 Web Profile 是否把 Mirage 声明为启动 Bundle；依赖存在但未启用不拦截。
pub fn mirage_bundle_enabled(profile_directory: &Path) -> Result<bool, String> {
    let manifest_path = profile_directory.join("package.json");
    let content = match fs::read_to_string(&manifest_path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!("无法读取 Web Profile 清单：{error}"));
        }
    };
    let manifest: Value = serde_json::from_str(&content)
        .map_err(|error| format!("Web Profile 清单格式无效：{error}"))?;
    let bundles = manifest
        .get("dsh")
        .and_then(|value| value.get("profile"))
        .and_then(|value| value.get("bundles"))
        .and_then(Value::as_array);

    Ok(bundles.is_some_and(|items| {
        items
            .iter()
            .any(|item| item.as_str() == Some(MIRAGE_BUNDLE))
    }))
}
#[cfg(test)]
mod tests {
    use std::{ffi::OsStr, fs, path::PathBuf};

    use super::{mirage_bundle_enabled, resolve_dsh_home};

    fn profile_fixture(name: &str, manifest: Option<&str>) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("dsh-desktop-profile-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("应创建测试 Profile");
        if let Some(manifest) = manifest {
            fs::write(directory.join("package.json"), manifest).expect("应写入测试清单");
        }
        directory
    }

    #[test]
    fn active_mirage_bundle_requires_the_desktop_compatibility_patch() {
        let profile = profile_fixture(
            "mirage-bundle",
            Some(
                r#"{"dependencies":{"@struktoai/mirage-dsh":"0.0.1"},"dsh":{"profile":{"bundles":["@deepseek-ai/dsh-base","@deepseek-ai/dsh-web-app","@struktoai/mirage-dsh"]}}}"#,
            ),
        );

        assert!(mirage_bundle_enabled(&profile).expect("有效清单应可检查"));
        fs::remove_dir_all(profile).expect("应清理测试 Profile");
    }

    #[test]
    fn dependency_without_an_active_bundle_keeps_the_native_profile() {
        let profile = profile_fixture(
            "dependency-only",
            Some(r#"{"dependencies":{"@struktoai/mirage-dsh":"0.0.1"}}"#),
        );

        assert!(!mirage_bundle_enabled(&profile).expect("有效清单应可检查"));
        fs::remove_dir_all(profile).expect("应清理测试 Profile");
    }

    #[test]
    fn dsh_home_resolution_honors_blank_relative_and_tilde_overrides() {
        let root = std::env::temp_dir().join("dsh-desktop-home-resolution");
        let home = root.join("home-root");
        let current = root.join("current-root");

        assert_eq!(
            resolve_dsh_home(&home, &current, Some(OsStr::new("   "))),
            home.join(".dsh")
        );
        assert_eq!(
            resolve_dsh_home(&home, &current, Some(OsStr::new("custom-dsh"))),
            current.join("custom-dsh")
        );
        assert_eq!(
            resolve_dsh_home(&home, &current, Some(OsStr::new("~/custom-dsh"))),
            home.join("custom-dsh")
        );
    }

    #[test]
    fn missing_profile_manifest_keeps_the_native_profile() {
        let profile = profile_fixture("missing", None);

        assert!(!mirage_bundle_enabled(&profile).expect("缺少清单不是错误"));
        fs::remove_dir_all(profile).expect("应清理测试 Profile");
    }
}
