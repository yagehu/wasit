pub mod fs;
pub mod mem;

pub trait SnapshotStore {
    type Snapshot;
    type Error;

    fn push_snapshot(&mut self, snapshot: Self::Snapshot) -> Result<(), Self::Error>;
    fn snapshot_count(&self) -> usize;
    fn get_snapshot(&self, idx: usize) -> Result<Option<Self::Snapshot>, Self::Error>;
}
