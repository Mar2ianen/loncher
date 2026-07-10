#![forbid(unsafe_code)]

use std::collections::HashMap;

use loncher_sync_protocol::{DeviceId, OperationId, SyncCursor, SyncOperation, SyncProtocolError};
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
    operations_by_id: HashMap<OperationId, SyncOperation>,
    highest_sequence: HashMap<DeviceId, u64>,
}

/// Test and bootstrap implementation of the replication engine.
///
/// Production transports and persistent storage are deliberately out of scope
/// until the launcher and daemon are useful. This engine fixes the semantics we
/// need now: validation, idempotent operation IDs and gap-free per-device
/// sequences.
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
        let mut staged_by_id = state.operations_by_id.clone();
        let mut staged_highest = state.highest_sequence.clone();
        let mut accepted_operations = Vec::new();
        let mut duplicates = 0;

        for operation in operations {
            if let Some(existing) = staged_by_id.get(&operation.operation_id) {
                if existing == &operation {
                    duplicates += 1;
                    continue;
                }

                return Err(SyncEngineError::OperationIdCollision {
                    operation_id: operation.operation_id,
                });
            }

            let current = staged_highest.get(&operation.device_id).copied().unwrap_or_default();
            let expected = current.saturating_add(1);

            if operation.device_sequence != expected {
                return Err(SyncEngineError::UnexpectedDeviceSequence {
                    device_id: operation.device_id,
                    expected,
                    actual: operation.device_sequence,
                });
            }

            staged_highest.insert(operation.device_id.clone(), operation.device_sequence);
            staged_by_id.insert(operation.operation_id.clone(), operation.clone());
            accepted_operations.push(operation);
        }

        let accepted = accepted_operations.len();
        state.operations_by_id = staged_by_id;
        state.highest_sequence = staged_highest;
        state.operations.extend(accepted_operations);

        Ok(SyncAck { accepted, duplicates })
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

        Ok(PullBatch { operations, cursor: next_cursor })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SyncEngineError {
    #[error(transparent)]
    Protocol(#[from] SyncProtocolError),
    #[error("pull limit must be greater than zero")]
    ZeroPullLimit,
    #[error("operation ID {operation_id} was reused for different content")]
    OperationIdCollision { operation_id: OperationId },
    #[error("unexpected sequence for device {device_id}: expected {expected}, got {actual}")]
    UnexpectedDeviceSequence { device_id: DeviceId, expected: u64, actual: u64 },
}

#[cfg(test)]
mod tests {
    use loncher_sync_protocol::{
        DeviceId, EntityKey, OperationId, Revision, SYNC_SCHEMA_VERSION, SyncCursor, SyncOperation,
        SyncPayload,
    };

    use super::{InMemorySyncEngine, SyncEngineError};

    fn operation(device: &str, id: &str, sequence: u64) -> SyncOperation {
        let device_id = DeviceId::new(device).expect("valid fixture device ID");
        SyncOperation {
            schema_version: SYNC_SCHEMA_VERSION,
            operation_id: OperationId::new(id).expect("valid fixture operation ID"),
            device_id: device_id.clone(),
            device_sequence: sequence,
            entity: EntityKey::new("settings", format!("key-{sequence}"))
                .expect("valid fixture entity key"),
            payload: SyncPayload::Put {
                revision: Revision { device_id, counter: sequence },
                ciphertext: vec![sequence as u8],
            },
        }
    }

    #[tokio::test]
    async fn push_is_idempotent_for_identical_operation() {
        let engine = InMemorySyncEngine::default();
        let first = operation("desktop", "same", 1);

        let ack = engine.push_batch(vec![first.clone(), first]).await.expect("valid batch");

        assert_eq!(ack.accepted, 1);
        assert_eq!(ack.duplicates, 1);
    }

    #[tokio::test]
    async fn rejects_operation_id_collision() {
        let engine = InMemorySyncEngine::default();
        let first = operation("desktop", "same", 1);
        let mut conflicting = first.clone();
        conflicting.entity =
            EntityKey::new("settings", "different").expect("valid fixture entity key");

        let error = engine
            .push_batch(vec![first, conflicting])
            .await
            .expect_err("conflicting operation ID must fail");

        assert!(matches!(error, SyncEngineError::OperationIdCollision { .. }));
    }

    #[tokio::test]
    async fn rejects_gap_in_device_sequence() {
        let engine = InMemorySyncEngine::default();

        let error = engine
            .push_batch(vec![operation("desktop", "two", 2)])
            .await
            .expect_err("sequence gap must fail");

        assert_eq!(
            error,
            SyncEngineError::UnexpectedDeviceSequence {
                device_id: DeviceId::new("desktop").expect("valid fixture device ID"),
                expected: 1,
                actual: 2,
            }
        );
    }

    #[tokio::test]
    async fn rejected_batch_does_not_partially_mutate_state() {
        let engine = InMemorySyncEngine::default();

        engine
            .push_batch(vec![operation("desktop", "one", 1), operation("desktop", "three", 3)])
            .await
            .expect_err("batch with a gap must fail");

        let pulled =
            engine.pull_after(&SyncCursor::default(), 10).await.expect("pull must succeed");

        assert!(pulled.operations.is_empty());
    }

    #[tokio::test]
    async fn pull_advances_per_device_cursor_without_skipping() {
        let engine = InMemorySyncEngine::default();
        engine
            .push_batch(vec![
                operation("desktop", "desktop-one", 1),
                operation("laptop", "laptop-one", 1),
                operation("desktop", "desktop-two", 2),
            ])
            .await
            .expect("valid batch");

        let first = engine.pull_after(&SyncCursor::default(), 1).await.expect("valid pull");
        let second = engine.pull_after(&first.cursor, 10).await.expect("valid pull");

        assert_eq!(first.operations.len(), 1);
        assert_eq!(second.operations.len(), 2);
        assert_eq!(second.operations[0].device_id.as_str(), "laptop");
        assert_eq!(second.operations[1].device_sequence, 2);
    }
}
