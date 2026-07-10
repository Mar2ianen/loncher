use std::io;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;
use tokio::net::UnixStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

pub type JsonFramed = Framed<UnixStream, LengthDelimitedCodec>;

pub fn framed(stream: UnixStream, max_frame_size: usize) -> JsonFramed {
    let codec = LengthDelimitedCodec::builder()
        .max_frame_length(max_frame_size)
        .new_codec();
    Framed::new(stream, codec)
}

pub async fn send_json<T>(framed: &mut JsonFramed, value: &T) -> Result<(), TransportError>
where
    T: Serialize,
{
    let payload = serde_json::to_vec(value)?;
    framed.send(Bytes::from(payload)).await?;
    Ok(())
}

pub async fn receive_json<T>(framed: &mut JsonFramed) -> Result<T, TransportError>
where
    T: DeserializeOwned,
{
    let frame = framed
        .next()
        .await
        .ok_or(TransportError::ConnectionClosed)??;
    Ok(serde_json::from_slice(&frame)?)
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("connection closed before a complete frame was received")]
    ConnectionClosed,
    #[error("socket transport failed: {0}")]
    Io(#[from] io::Error),
    #[error("JSON framing failed: {0}")]
    Json(#[from] serde_json::Error),
}
