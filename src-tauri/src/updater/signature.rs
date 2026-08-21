use std::{fs::File, io::Read, path::Path};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use minisign_verify::{Error as MinisignError, PublicKey, Signature};

use super::download::DownloadError;

/// 复用Tauri Updater的公钥和签名解码方式，并优先流式校验现代预哈希签名。
pub fn verify_signature_file(
    path: &Path,
    release_signature: &str,
    public_key: &str,
) -> Result<(), DownloadError> {
    let (public_key, signature) = decode_material(release_signature, public_key)?;
    match public_key.verify_stream(&signature) {
        Ok(mut verifier) => {
            let mut file = File::open(path)?;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                verifier.update(&buffer[..read]);
            }
            verifier
                .finalize()
                .map_err(|error| DownloadError::Signature(error.to_string()))
        }
        Err(MinisignError::UnsupportedLegacyMode) => {
            let data = std::fs::read(path)?;
            public_key
                .verify(&data, &signature, true)
                .map_err(|error| DownloadError::Signature(error.to_string()))
        }
        Err(error) => Err(DownloadError::Signature(error.to_string())),
    }
}

fn decode_material(
    release_signature: &str,
    public_key: &str,
) -> Result<(PublicKey, Signature), DownloadError> {
    let public_key = decode_base64_text(public_key, "公钥")?;
    let public_key = PublicKey::decode(&public_key)
        .map_err(|error| DownloadError::Signature(error.to_string()))?;
    let signature = decode_base64_text(release_signature, "签名")?;
    let signature = Signature::decode(&signature)
        .map_err(|error| DownloadError::Signature(error.to_string()))?;
    Ok((public_key, signature))
}

fn decode_base64_text(value: &str, label: &str) -> Result<String, DownloadError> {
    let decoded = BASE64_STANDARD
        .decode(value)
        .map_err(|error| DownloadError::Signature(format!("{label}Base64无效：{error}")))?;
    String::from_utf8(decoded)
        .map_err(|error| DownloadError::Signature(format!("{label}不是UTF-8文本：{error}")))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};

    use super::verify_signature_file;

    const PUBLIC_KEY: &str = "untrusted comment: minisign public key E7620F1842B4E81F\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    const SIGNATURE: &str = "untrusted comment: signature from minisign secret key\nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\ntrusted comment: timestamp:1556193335\tfile:test\ny/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==";

    #[test]
    fn file_signature_verification_streams_prehashed_minisign_data() {
        let path =
            std::env::temp_dir().join(format!("dsh-desktop-signature-{}.bin", std::process::id()));
        fs::write(&path, b"test").expect("应写入签名测试文件");
        let public_key = BASE64_STANDARD.encode(PUBLIC_KEY);
        let signature = BASE64_STANDARD.encode(SIGNATURE);

        verify_signature_file(&path, &signature, &public_key).expect("正确文件应通过验签");
        fs::write(&path, b"Test").expect("应篡改测试文件");
        assert!(verify_signature_file(&path, &signature, &public_key).is_err());
        fs::remove_file(path).expect("应清理测试文件");
    }
}
