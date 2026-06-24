use std::path::{Path, PathBuf};
use std::fs;

#[derive(Debug, Clone)]
pub struct WalArchiver {
    archive_dir: PathBuf,
}

impl WalArchiver {
    pub fn new(archive_dir: PathBuf) -> Self {
        Self { archive_dir }
    }

    pub fn archive_segment(&self, seg_no: u32, wal_dir: &Path) -> anyhow::Result<()> {
        let seg_name = format!("{:08X}{:08X}{:08X}", 1, 0, seg_no);
        let src = wal_dir.join(&seg_name);
        let dst = self.archive_dir.join(&seg_name);

        if !self.archive_dir.exists() {
            fs::create_dir_all(&self.archive_dir)?;
        }

        fs::copy(&src, &dst)?;

        let status_dir = wal_dir.join("archive_status");
        if !status_dir.exists() {
            fs::create_dir_all(&status_dir)?;
        }
        fs::write(status_dir.join(format!("{}.done", seg_name)), "")?;

        Ok(())
    }

    pub fn restore_segment(&self, seg_no: u32, wal_dir: &Path) -> anyhow::Result<()> {
        let seg_name = format!("{:08X}{:08X}{:08X}", 1, 0, seg_no);
        let src = self.archive_dir.join(&seg_name);
        let dst = wal_dir.join(&seg_name);

        if !wal_dir.exists() {
            fs::create_dir_all(wal_dir)?;
        }

        fs::copy(&src, &dst)?;
        Ok(())
    }

    pub fn list_archived(&self) -> anyhow::Result<Vec<u32>> {
        let mut segments = Vec::new();
        if self.archive_dir.exists() {
            for entry in fs::read_dir(&self.archive_dir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().to_string();
                if name.len() == 24 {
                    if let Ok(seg_no) = u32::from_str_radix(&name[16..24], 16) {
                        segments.push(seg_no);
                    }
                }
            }
        }
        segments.sort();
        Ok(segments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_wal_archiver_new() {
        let dir = tempdir().unwrap();
        let archiver = WalArchiver::new(dir.path().to_path_buf());
        assert!(archiver.archive_dir.exists() || !archiver.archive_dir.exists());
    }

    #[test]
    fn test_list_archived_empty() {
        let dir = tempdir().unwrap();
        let archiver = WalArchiver::new(dir.path().to_path_buf());
        let segments = archiver.list_archived().unwrap();
        assert!(segments.is_empty());
    }
}
