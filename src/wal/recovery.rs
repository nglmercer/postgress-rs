use crate::types::{Oid, PageId};
use crate::storage::StorageTrait;
use crate::wal::{ControlFile, WALRecord};
use std::sync::Arc;
use std::path::Path;

pub struct WalRecovery {
    control: ControlFile,
    storage: Arc<dyn StorageTrait>,
}

#[derive(Debug)]
pub struct RecoveryResult {
    pub replayed_lsn: u64,
    pub records_replayed: u64,
    pub consistent: bool,
}

impl WalRecovery {
    pub fn new(control: ControlFile, storage: Arc<dyn StorageTrait>) -> Self {
        Self { control, storage }
    }

    pub fn from_control_file(path: &Path, storage: Arc<dyn StorageTrait>) -> anyhow::Result<Self> {
        let data = std::fs::read(path)?;
        let control = ControlFile::deserialize(&data);
        Ok(Self { control, storage })
    }

    pub fn save_control_file(&self, path: &Path) -> anyhow::Result<()> {
        let data = self.control.serialize();
        std::fs::write(path, &data)?;
        Ok(())
    }

    pub async fn recover(&mut self) -> anyhow::Result<RecoveryResult> {
        let mut replayed_lsn = self.control.redo_lsn;
        let mut records_replayed = 0u64;

        let segment_size = 16 * 1024 * 1024;
        let start_segment = (replayed_lsn / segment_size as u64) as u32;

        for seg in start_segment..start_segment + 10 {
            let page_id = PageId(seg + 1);
            let page_data = match self.storage.read_page(page_id) {
                Ok(data) => data,
                Err(_) => break,
            };

            let mut offset = 0;
            while offset < page_data.len() {
                let record = match bincode::deserialize::<WALRecord>(&page_data[offset..]) {
                    Ok(r) => r,
                    Err(_) => break,
                };

                let data_len = bincode::serialize(&record).map(|d| d.len()).unwrap_or(0);
                if data_len == 0 {
                    break;
                }

                let lsn = (seg as u64) * segment_size as u64 + offset as u64;
                if lsn >= self.control.redo_lsn {
                    self.apply_record(&record)?;
                    records_replayed += 1;
                }

                offset += data_len;
                replayed_lsn = lsn + data_len as u64;
            }
        }

        self.control.redo_lsn = replayed_lsn;
        self.control.check_point_lsn = replayed_lsn;

        Ok(RecoveryResult {
            replayed_lsn,
            records_replayed,
            consistent: true,
        })
    }

    fn apply_record(&self, record: &WALRecord) -> anyhow::Result<()> {
        match record {
            WALRecord::Begin { .. } | WALRecord::Commit { .. } | WALRecord::Abort { .. } => {}
            WALRecord::Insert { .. } | WALRecord::Update { .. } | WALRecord::Delete { .. } => {}
            WALRecord::Checkpoint { .. } => {}
        }
        Ok(())
    }

    pub fn get_control(&self) -> &ControlFile {
        &self.control
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ephemeral::EphemeralStorage;

    #[test]
    fn test_recovery_result() {
        let result = RecoveryResult {
            replayed_lsn: 1000,
            records_replayed: 50,
            consistent: true,
        };
        assert_eq!(result.replayed_lsn, 1000);
        assert!(result.consistent);
    }

    #[test]
    fn test_wal_recovery_new() {
        let storage = Arc::new(EphemeralStorage::new());
        let control = ControlFile::create(12345);
        let recovery = WalRecovery::new(control, storage);
        assert_eq!(recovery.get_control().system_identifier, 12345);
    }
}
