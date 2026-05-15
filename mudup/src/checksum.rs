use anyhow::{Result, bail};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;

pub(crate) fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let actual = format!("{:x}", hasher.finalize());
    let expected = expected.trim().to_ascii_lowercase();
    if actual != expected {
        let _ = fs::remove_file(path);
        bail!(
            "checksum mismatch for {}: expected {}, got {}",
            path.display(),
            expected,
            actual
        );
    }

    Ok(())
}
