use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const PART_FILE_NAME: &str = "package.part";
const METADATA_FILE_NAME: &str = "package.json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct DownloadIdentity {
    pub version: String,
    pub url: String,
    pub signature: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DownloadMetadata {
    identity: DownloadIdentity,
    expected_total: Option<u64>,
}

#[derive(Debug, Error)]
pub(super) enum CacheError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("更新缓存路径不是普通文件或目录：{0}")]
    UnsafePath(PathBuf),
    #[error("更新缓存超过安全上限：{size} 字节")]
    Oversized { size: u64 },
}

/// 保存单个待安装版本的下载文件；新版本或新签名会自动清理旧缓存。
pub(super) struct DownloadCache {
    part_path: PathBuf,
    metadata_path: PathBuf,
    metadata: DownloadMetadata,
    downloaded_len: u64,
}

impl DownloadCache {
    pub fn prepare(
        directory: &Path,
        identity: DownloadIdentity,
        max_bytes: u64,
    ) -> Result<Self, CacheError> {
        ensure_directory(directory)?;
        let part_path = directory.join(PART_FILE_NAME);
        let metadata_path = directory.join(METADATA_FILE_NAME);
        ensure_regular_file_if_present(&part_path)?;
        ensure_regular_file_if_present(&metadata_path)?;

        let existing = read_metadata(&metadata_path).ok();
        if existing
            .as_ref()
            .is_some_and(|metadata| metadata.identity != identity)
            || existing.is_none() && (part_path.exists() || metadata_path.exists())
        {
            remove_if_present(&part_path)?;
            remove_if_present(&metadata_path)?;
        }

        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&part_path)?;
        let mut downloaded_len = fs::metadata(&part_path)?.len();
        if downloaded_len > max_bytes {
            return Err(CacheError::Oversized {
                size: downloaded_len,
            });
        }
        let mut metadata = existing
            .filter(|metadata| metadata.identity == identity)
            .unwrap_or(DownloadMetadata {
                identity,
                expected_total: None,
            });
        if metadata
            .expected_total
            .is_some_and(|total| downloaded_len > total)
        {
            remove_if_present(&part_path)?;
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&part_path)?;
            downloaded_len = 0;
            metadata.expected_total = None;
        }
        write_metadata(&metadata_path, &metadata)?;

        Ok(Self {
            part_path,
            metadata_path,
            metadata,
            downloaded_len,
        })
    }

    pub fn part_path(&self) -> &Path {
        &self.part_path
    }

    pub fn downloaded_len(&self) -> u64 {
        self.downloaded_len
    }

    pub fn set_downloaded_len(&mut self, downloaded_len: u64) {
        self.downloaded_len = downloaded_len;
    }

    pub fn expected_total(&self) -> Option<u64> {
        self.metadata.expected_total
    }

    pub fn set_expected_total(&mut self, total: Option<u64>) -> Result<(), CacheError> {
        self.metadata.expected_total = total;
        write_metadata(&self.metadata_path, &self.metadata)
    }

    pub fn clear(self) -> Result<(), CacheError> {
        remove_if_present(&self.part_path)?;
        remove_if_present(&self.metadata_path)?;
        Ok(())
    }
}

fn ensure_directory(directory: &Path) -> Result<(), CacheError> {
    fs::create_dir_all(directory)?;
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(CacheError::UnsafePath(directory.to_path_buf()));
    }
    Ok(())
}

fn ensure_regular_file_if_present(path: &Path) -> Result<(), CacheError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(CacheError::UnsafePath(path.to_path_buf()));
    }
    Ok(())
}

fn read_metadata(path: &Path) -> Result<DownloadMetadata, CacheError> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn write_metadata(path: &Path, metadata: &DownloadMetadata) -> Result<(), CacheError> {
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    remove_if_present(&temporary)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(serde_json::to_string(metadata)?.as_bytes())?;
    file.sync_all()?;
    drop(file);
    remove_if_present(path)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn remove_if_present(path: &Path) -> Result<(), std::io::Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::{DownloadCache, DownloadIdentity};

    fn test_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dsh-desktop-updater-cache-{name}-{}",
            std::process::id()
        ))
    }

    fn identity(version: &str) -> DownloadIdentity {
        DownloadIdentity {
            version: version.to_string(),
            url: format!("https://example.com/{version}.exe"),
            signature: format!("signature-{version}"),
        }
    }

    #[test]
    fn matching_identity_preserves_partial_bytes_and_total() {
        let directory = test_directory("preserve");
        let _ = fs::remove_dir_all(&directory);
        let mut cache =
            DownloadCache::prepare(&directory, identity("0.1.3"), 1024).expect("首次缓存应可创建");
        fs::write(cache.part_path(), b"partial").expect("应写入部分下载");
        cache.set_expected_total(Some(128)).expect("应保存总大小");

        let cache = DownloadCache::prepare(&directory, identity("0.1.3"), 1024)
            .expect("同一更新应恢复缓存");

        assert_eq!(cache.downloaded_len(), 7);
        assert_eq!(cache.expected_total(), Some(128));
        fs::remove_dir_all(directory).expect("应清理测试目录");
    }

    #[test]
    fn changed_identity_discards_stale_partial_bytes() {
        let directory = test_directory("replace");
        let _ = fs::remove_dir_all(&directory);
        let cache =
            DownloadCache::prepare(&directory, identity("0.1.3"), 1024).expect("首次缓存应可创建");
        fs::write(cache.part_path(), b"stale").expect("应写入旧缓存");

        let cache = DownloadCache::prepare(&directory, identity("0.1.4"), 1024)
            .expect("新版本应替换旧缓存");

        assert_eq!(cache.downloaded_len(), 0);
        fs::remove_dir_all(directory).expect("应清理测试目录");
    }

    #[test]
    fn partial_file_larger_than_the_recorded_total_is_reset() {
        let directory = test_directory("corrupted-size");
        let _ = fs::remove_dir_all(&directory);
        let mut cache =
            DownloadCache::prepare(&directory, identity("0.1.3"), 1024).expect("首次缓存应可创建");
        fs::write(cache.part_path(), b"too-large").expect("应写入异常缓存");
        cache.set_expected_total(Some(4)).expect("应保存声明大小");

        let cache = DownloadCache::prepare(&directory, identity("0.1.3"), 1024)
            .expect("异常缓存应自动恢复");

        assert_eq!(cache.downloaded_len(), 0);
        assert_eq!(cache.expected_total(), None);
        fs::remove_dir_all(directory).expect("应清理测试目录");
    }
}
