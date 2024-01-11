use serde::{Deserialize, Serialize};

use crate::Value;

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub errno:   Option<i32>,
    pub results: Vec<CallResult>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CallResult {
    pub memory_offset: u32,
    pub value:         Value,
}

pub trait SnapshotHandler {
    fn process_snapshot(&mut self, idx: usize, snapshot: &Snapshot);
}

#[derive(Debug)]
pub struct Recorder<'sh, SH> {
    snapshot_handler:  &'sh mut SH,
    next_snapshot_idx: usize,
}

impl<'sh, SH> Recorder<'sh, SH>
where
    SH: SnapshotHandler,
{
    pub fn new(snapshot_handler: &'sh mut SH) -> Self {
        Self {
            snapshot_handler,
            next_snapshot_idx: 0,
        }
    }

    pub fn take_snapshot(&mut self, errno: Option<i32>, results: Vec<CallResult>) {
        let idx = self.next_snapshot_idx;

        self.next_snapshot_idx += 1;

        let snapshot = Snapshot { errno, results };

        self.snapshot_handler.process_snapshot(idx, &snapshot);
    }
}

#[derive(Default, Clone, Debug)]
pub struct InMemorySnapshots {
    pub snapshots: Vec<Snapshot>,
}

impl SnapshotHandler for InMemorySnapshots {
    fn process_snapshot(&mut self, _idx: usize, snapshot: &Snapshot) {
        self.snapshots.push(snapshot.to_owned());
    }
}
