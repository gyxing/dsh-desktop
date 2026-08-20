use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};

use super::download::DownloadError;

/// 完全复用Tauri Updater 2.10.1的公钥、签名解码方式和legacy校验参数。
pub fn verify_signature(
    data: &[u8],
    release_signature: &str,
    public_key: &str,
) -> Result<(), DownloadError> {
    let public_key = decode_base64_text(public_key, "公钥")?;
    let public_key = PublicKey::decode(&public_key)
        .map_err(|error| DownloadError::Signature(error.to_string()))?;
    let signature = decode_base64_text(release_signature, "签名")?;
    let signature = Signature::decode(&signature)
        .map_err(|error| DownloadError::Signature(error.to_string()))?;
    public_key
        .verify(data, &signature, true)
        .map_err(|error| DownloadError::Signature(error.to_string()))?;
    Ok(())
}

fn decode_base64_text(value: &str, label: &str) -> Result<String, DownloadError> {
    let decoded = BASE64_STANDARD
        .decode(value)
        .map_err(|error| DownloadError::Signature(format!("{label}Base64无效：{error}")))?;
    String::from_utf8(decoded)
        .map_err(|error| DownloadError::Signature(format!("{label}不是UTF-8文本：{error}")))
}
