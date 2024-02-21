use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
};

use super::SnapshotStore;

#[derive(Debug, Clone)]
pub struct InMemorySnapshotStore<S> {
    snapshots: Arc<Mutex<Vec<S>>>,
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

    fn push_snapshot(&self, snapshot: Self::Snapshot) -> Result<(), Self::Error> {
        self.snapshots.lock().unwrap().push(snapshot);

        Ok(())
    }

    fn snapshot_count(&self) -> usize {
        self.snapshots.lock().unwrap().len()
    }

    fn get_snapshot(&self, idx: usize) -> Result<Option<Self::Snapshot>, Self::Error> {
        Ok(self.snapshots.lock().unwrap().get(idx).cloned())
    }
}
