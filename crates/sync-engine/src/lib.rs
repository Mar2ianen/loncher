#![forbid(unsafe_code)]

use std::collections::HashSet;

use loncher_sync_protocol::{OperationId, SyncCursor, SyncOperation, SyncProtocolError};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncAck {
    pub accepted: usize,
    pub duplicates: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullBatch {
    pub operations: Vec<SyncOperation>,
    pub cursor: SyncCursor,
}

#[derive(Debug, Default)]
struct InMemoryState {
    operations: Vec<SyncOperation>,
    seen: HashSet<OperationId>,
}

/// Test and bootstrap implementation of the replication engine.
///
/// Production transports and persistent storage are deliberately out of scope
/// until the launcher and daemon are useful. This engine fixes the semantics we
/// need now: validation, idempotent operation IDs and per-device cursors.
#[derive(Debug, Default)]
pub struct InMemorySyncEngine {
    state: RwLock<InMemoryState>,
}

impl InMemorySyncEngine {
    pub async fn push_batch(
        &self,
        operations: Vec<SyncOperation>,
    ) -> Result<SyncAck, SyncEngineError> {
        for operation in &operations {
            operation.validate()?;
        }

        let mut state = self.state.write().await;
        let mut accepted = 0;
        let mut duplicates = 0;

        for operation in operations {
            if state.seen.insert(operation.operation_id.clone()) {
                state.operations.push(operation);
                accepted += 1;
            } else {
                duplicates += 1;
            }
        }

        Ok(SyncAck {
            accepted,
            duplicates,
        })
    }

    pub async fn pull_after(
        &self,
        cursor: &SyncCursor,
        limit: usize,
    ) -> Result<PullBatch, SyncEngineError> {
        if limit == 0 {
            return Err(SyncEngineError::ZeroPullLimit);
        }

        let state = self.state.read().await;
        let operations: Vec<_> = state
            .operations
            .iter()
            .filter(|operation| {
                operation.device_sequence > cursor.sequence_for(&operation.device_id)
            })
            .take(limit)
            .cloned()
            .collect();

        let mut next_cursor = cursor.clone();
        for operation in &operations {
            next_cursor.advance(operation.device_id.clone(), operation.device_sequence);
        }

        Ok(PullBatch {
            operations,
            cursor: next_cursor,
        })
    }
}

#[derive(Debug, Error)]
pub enum SyncEngineError {
    #[error(transparent)]
    Protocol(#[from] SyncProtocolError),
    #[error("pull limit must be greater than zero")]
    ZeroPullLimit,
}

#[cfg(test)]
mod tests {
    use loncher_sync_protocol::{
        DeviceId, EntityKey, OperationId, Revision, SYNC_SCHEMA_VERSION, SyncCursor, SyncOperation,
        SyncPayload,
    };

    use super::InMemorySyncEngine;

    fn operation(id: &str, sequence: u64) -> SyncOperation {
        let device_id = DeviceId::new("desktop").expect("valid fixture device ID");
        SyncOperation {
            schema_version: SYNC_SCHEMA_VERSION,
            operation_id: OperationId::new(id).expect("valid fixture operation ID"),
            device_id: device_id.clone(),
            device_sequence: sequence,
            entity: EntityKey::new("settings", format!("key-{sequence}"))
                .expect("valid fixture entity key"),
            payload: SyncPayload::Put {
                revision: Revision {
                    device_id,
                    counter: sequence,
                },
                ciphertext: vec![sequence as u8],
            },
        }
    }

    #[tokio::test]
    async fn push_is_idempotent_by_operation_id() {
        let engine = InMemorySyncEngine::default();
        let first = operation("same", 1);

        let ack = engine
            .push_batch(vec![first.clone(), first])
            .await
            .expect("valid batch");

        assert_eq!(ack.accepted, 1);
        assert_eq!(ack.duplicates, 1);
    }

    #[tokio::test]
    async fn pull_advances_per_device_cursor() {
        let engine = InMemorySyncEngine::default();
        engine
            .push_batch(vec![operation("one", 1), operation("two", 2)])
            .await
            .expect("valid batch");

        let first = engine
            .pull_after(&SyncCursor::default(), 1)
            .await
            .expect("valid pull");
        let second = engine
            .pull_after(&first.cursor, 10)
            .await
            .expect("valid pull");

        assert_eq!(first.operations.len(), 1);
        assert_eq!(second.operations.len(), 1);
        assert_eq!(second.operations[0].device_sequence, 2);
    }
}
