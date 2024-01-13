use std::convert::Infallible;

use super::SnapshotStore;

#[derive(Debug, Clone)]
pub struct InMemorySnapshotStore<S> {
    snapshots: Vec<S>,
}

impl<S> Default for InMemorySnapshotStore<S> {
    fn default() -> Self {
        Self {
            snapshots: Default::default(),
        }
    }
}

impl<S> SnapshotStore for InMemorySnapshotStore<S>
where
    S: Clone,
{
    type Snapshot = S;
    type Error = Infallible;

    fn push_snapshot(&mut self, snapshot: Self::Snapshot) -> Result<(), Self::Error> {
        self.snapshots.push(snapshot);

        Ok(())
    }

    fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    fn get_snapshot(&self, idx: usize) -> Result<Option<Self::Snapshot>, Self::Error> {
        Ok(self.snapshots.get(idx).cloned())
    }
}
