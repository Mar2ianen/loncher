#![forbid(unsafe_code)]

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Deserializer, Serialize, de};
use thiserror::Error;

pub const SYNC_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(value: impl Into<String>) -> Result<Self, SyncProtocolError> {
        let value = value.into();
        validate_identifier("device_id", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DeviceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct OperationId(String);

impl OperationId {
    pub fn new(value: impl Into<String>) -> Result<Self, SyncProtocolError> {
        let value = value.into();
        validate_identifier("operation_id", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for OperationId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct EntityKey {
    namespace: String,
    key: String,
}

impl EntityKey {
    pub fn new(
        namespace: impl Into<String>,
        key: impl Into<String>,
    ) -> Result<Self, SyncProtocolError> {
        let namespace = namespace.into();
        let key = key.into();
        validate_identifier("entity namespace", &namespace)?;
        validate_identifier("entity key", &key)?;
        Ok(Self { namespace, key })
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

impl<'de> Deserialize<'de> for EntityKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireEntityKey {
            namespace: String,
            key: String,
        }

        let value = WireEntityKey::deserialize(deserializer)?;
        Self::new(value.namespace, value.key).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub device_id: DeviceId,
    pub counter: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SyncPayload {
    Put {
        revision: Revision,
        /// Opaque application-level encrypted bytes. The sync hub must not
        /// interpret this payload.
        ciphertext: Vec<u8>,
    },
    Delete {
        revision: Revision,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncOperation {
    pub schema_version: u16,
    pub operation_id: OperationId,
    pub device_id: DeviceId,
    pub device_sequence: u64,
    pub entity: EntityKey,
    pub payload: SyncPayload,
}

impl SyncOperation {
    pub fn validate(&self) -> Result<(), SyncProtocolError> {
        if self.schema_version != SYNC_SCHEMA_VERSION {
            return Err(SyncProtocolError::UnsupportedSchemaVersion {
                expected: SYNC_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }

        if self.device_sequence == 0 {
            return Err(SyncProtocolError::ZeroDeviceSequence);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SyncCursor {
    sequences: BTreeMap<DeviceId, u64>,
}

impl SyncCursor {
    pub fn sequence_for(&self, device_id: &DeviceId) -> u64 {
        self.sequences.get(device_id).copied().unwrap_or_default()
    }

    pub fn advance(&mut self, device_id: DeviceId, sequence: u64) {
        self.sequences
            .entry(device_id)
            .and_modify(|current| *current = (*current).max(sequence))
            .or_insert(sequence);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergePolicy {
    LastWriterWins,
    GrowOnlySet,
    ObservedRemoveSet,
    AppendOnly,
    ManualConflict,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SyncProtocolError {
    #[error("{field} must not be empty")]
    EmptyIdentifier { field: &'static str },
    #[error("{field} is too long")]
    IdentifierTooLong { field: &'static str },
    #[error("unsupported sync schema version: expected {expected}, got {actual}")]
    UnsupportedSchemaVersion { expected: u16, actual: u16 },
    #[error("device sequence must start at 1")]
    ZeroDeviceSequence,
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), SyncProtocolError> {
    if value.trim().is_empty() {
        return Err(SyncProtocolError::EmptyIdentifier { field });
    }

    if value.len() > 256 {
        return Err(SyncProtocolError::IdentifierTooLong { field });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DeviceId, EntityKey, OperationId, Revision, SYNC_SCHEMA_VERSION, SyncOperation, SyncPayload,
        SyncProtocolError,
    };

    fn operation(sequence: u64) -> SyncOperation {
        let device_id = DeviceId::new("desktop").expect("valid fixture device ID");
        SyncOperation {
            schema_version: SYNC_SCHEMA_VERSION,
            operation_id: OperationId::new(format!("op-{sequence}"))
                .expect("valid fixture operation ID"),
            device_id: device_id.clone(),
            device_sequence: sequence,
            entity: EntityKey::new("settings", "theme").expect("valid fixture entity key"),
            payload: SyncPayload::Put {
                revision: Revision {
                    device_id,
                    counter: sequence,
                },
                ciphertext: vec![1, 2, 3],
            },
        }
    }

    #[test]
    fn rejects_zero_sequence() {
        assert_eq!(
            operation(0).validate(),
            Err(SyncProtocolError::ZeroDeviceSequence)
        );
    }

    #[test]
    fn accepts_current_schema() {
        assert_eq!(operation(1).validate(), Ok(()));
    }

    #[test]
    fn rejects_invalid_device_id_during_deserialization() {
        let error = serde_json::from_str::<DeviceId>(r#""""#).expect_err("empty ID must fail");
        assert!(error.to_string().contains("device_id must not be empty"));
    }

    #[test]
    fn rejects_invalid_entity_key_during_deserialization() {
        let error = serde_json::from_str::<EntityKey>(r#"{"namespace":"settings","key":" "}"#)
            .expect_err("empty key must fail");
        assert!(error.to_string().contains("entity key must not be empty"));
    }
}
