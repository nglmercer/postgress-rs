use postgress_rs::storage::ephemeral::EphemeralStorage;
use postgress_rs::storage::mmap::MmapStorage;
use postgress_rs::storage::StorageTrait;
use postgress_rs::types::PageId;
use std::sync::Arc;

#[test]
fn test_ephemeral_read_unwritten_page() {
    let storage = EphemeralStorage::new();
    let page = storage.read_page(PageId(999)).unwrap();
    assert_eq!(page.len(), 8192);
    assert!(page.iter().all(|&b| b == 0));
}

#[test]
fn test_ephemeral_write_read_roundtrip() {
    let storage = EphemeralStorage::new();
    let data = vec![42u8; 8192];
    storage.write_page(PageId(1), &data).unwrap();
    let read = storage.read_page(PageId(1)).unwrap();
    assert_eq!(read, data);
}

#[test]
fn test_ephemeral_multiple_pages() {
    let storage = EphemeralStorage::new();
    let data1 = vec![1u8; 8192];
    let data2 = vec![2u8; 8192];
    storage.write_page(PageId(1), &data1).unwrap();
    storage.write_page(PageId(2), &data2).unwrap();
    assert_eq!(storage.read_page(PageId(1)).unwrap(), data1);
    assert_eq!(storage.read_page(PageId(2)).unwrap(), data2);
}

#[test]
fn test_ephemeral_overwrite_page() {
    let storage = EphemeralStorage::new();
    storage.write_page(PageId(1), &vec![1u8; 8192]).unwrap();
    storage.write_page(PageId(1), &vec![2u8; 8192]).unwrap();
    assert_eq!(storage.read_page(PageId(1)).unwrap(), vec![2u8; 8192]);
}

#[test]
fn test_ephemeral_partial_page_data() {
    let storage = EphemeralStorage::new();
    let mut data = vec![0u8; 8192];
    data[0] = 0xFF;
    data[100] = 0xAB;
    storage.write_page(PageId(1), &data).unwrap();
    let read = storage.read_page(PageId(1)).unwrap();
    assert_eq!(read[0], 0xFF);
    assert_eq!(read[100], 0xAB);
    assert_eq!(read[5000], 0);
}

#[tokio::test]
async fn test_mmap_storage_open_and_read() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let storage = MmapStorage::open(tmp.path(), 8192).await.unwrap();
    let page = storage.read_page(PageId(0)).unwrap();
    assert_eq!(page.len(), 8192);
}

#[tokio::test]
async fn test_mmap_storage_write_read() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let storage = MmapStorage::open(tmp.path(), 8192).await.unwrap();
    let data = vec![0xABu8; 8192];
    storage.write_page(PageId(0), &data).unwrap();
    let read = storage.read_page(PageId(0)).unwrap();
    assert_eq!(read, data);
}

#[tokio::test]
async fn test_mmap_storage_multiple_pages() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let storage = MmapStorage::open(tmp.path(), 8192).await.unwrap();
    let data1 = vec![1u8; 8192];
    let data2 = vec![2u8; 8192];
    storage.write_page(PageId(0), &data1).unwrap();
    storage.write_page(PageId(1), &data2).unwrap();
    assert_eq!(storage.read_page(PageId(0)).unwrap(), data1);
    assert_eq!(storage.read_page(PageId(1)).unwrap(), data2);
}

#[tokio::test]
async fn test_mmap_storage_overwrite() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let storage = MmapStorage::open(tmp.path(), 8192).await.unwrap();
    storage.write_page(PageId(0), &vec![1u8; 8192]).unwrap();
    storage.write_page(PageId(0), &vec![2u8; 8192]).unwrap();
    assert_eq!(storage.read_page(PageId(0)).unwrap(), vec![2u8; 8192]);
}

#[tokio::test]
async fn test_mmap_storage_large_page_id() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let storage = MmapStorage::open(tmp.path(), 8192).await.unwrap();
    let data = vec![0xFFu8; 8192];
    storage.write_page(PageId(1000), &data).unwrap();
    let read = storage.read_page(PageId(1000)).unwrap();
    assert_eq!(read, data);
}

#[tokio::test]
async fn test_mmap_storage_as_trait_object() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let storage: Arc<dyn StorageTrait> =
        Arc::new(MmapStorage::open(tmp.path(), 8192).await.unwrap());
    let data = vec![42u8; 8192];
    storage.write_page(PageId(0), &data).unwrap();
    let read = storage.read_page(PageId(0)).unwrap();
    assert_eq!(read, data);
}

#[test]
fn test_ephemeral_as_trait_object() {
    let storage: Arc<dyn StorageTrait> = Arc::new(EphemeralStorage::new());
    let data = vec![99u8; 8192];
    storage.write_page(PageId(0), &data).unwrap();
    let read = storage.read_page(PageId(0)).unwrap();
    assert_eq!(read, data);
}
