use std::{
    fs,
    io,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::SnapshotStore;
use crate::snapshot::WasiSnapshot;

#[derive(Debug)]
pub struct FsSnapshotStore {
    root:  PathBuf,
    count: AtomicUsize,
}

impl FsSnapshotStore {
    const CALL_FILE_NAME: &'static str = "call.json";

    pub fn new(dir: PathBuf) -> Self {
        Self {
            root:  dir,
            count: AtomicUsize::new(0),
        }
    }

    fn idx_string(idx: usize) -> String {
        format!("{:06}", idx)
    }
}

impl SnapshotStore for FsSnapshotStore {
    type Snapshot = WasiSnapshot;
    type Error = io::Error;

    fn push_snapshot(&self, snapshot: WasiSnapshot) -> Result<(), Self::Error> {
        let idx = self.count.fetch_add(1, Ordering::SeqCst);
        let dir = self.root.join(Self::idx_string(idx));

        fs::create_dir(&dir)?;

        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(dir.join(Self::CALL_FILE_NAME))?;

        serde_json::to_writer_pretty(file, &snapshot)?;

        Ok(())
    }

    fn snapshot_count(&self) -> usize {
        fs::read_dir(&self.root).unwrap().count()
    }

    fn get_snapshot(&self, idx: usize) -> Result<Option<Self::Snapshot>, Self::Error> {
        if idx >= self.snapshot_count() {
            return Ok(None);
        }

        let dir = self.root.join(Self::idx_string(idx));
        let call_file = fs::OpenOptions::new()
            .read(true)
            .open(dir.join(Self::CALL_FILE_NAME))?;
        let snapshot: WasiSnapshot = serde_json::from_reader(call_file)?;

        Ok(Some(snapshot))
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn ok() {
        let root = tempdir().unwrap();
        let store = FsSnapshotStore::new(root.path().to_path_buf());
        let snapshot = WasiSnapshot { errno: Some(21) };

        store.push_snapshot(snapshot.clone()).unwrap();

        assert_eq!(store.snapshot_count(), 1);

        let snapshot_roundtrip = store.get_snapshot(0).unwrap().unwrap();

        assert_eq!(snapshot_roundtrip, snapshot);
        assert!(store.get_snapshot(1).unwrap().is_none());
    }
}
