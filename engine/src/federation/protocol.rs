//! Gossip-based protocol for triple synchronization.
//!
//! Replaces the old cursor-based request-response with a 3-step bloom filter
//! gossip protocol:
//! 1. Exchange bloom filters of triple hashes
//! 2. Request/send headers for potentially missing triples
//! 3. Request/send full signed triples

use std::io;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use libp2p::request_response;
use libp2p::StreamProtocol;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Triple;

/// Protocol name for the gossip sync protocol.
pub const PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/valence/gossip-sync/2.0.0");

/// Maximum message size (16 MB) to prevent unbounded allocations.
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Messages exchanged during the bloom filter gossip protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessage {
    /// Step 1: Exchange bloom filter of triple hashes
    BloomExchange {
        filter: Vec<u8>,
        triple_count: u64,
    },
    /// Step 2: Request headers for triples we might want
    HeaderRequest {
        triple_hashes: Vec<[u8; 32]>,
    },
    /// Step 2 response: Triple headers (lightweight, no signatures)
    Headers {
        headers: Vec<TripleHeader>,
    },
    /// Step 3: Request full triples
    TripleRequest {
        triple_ids: Vec<Uuid>,
    },
    /// Step 3 response: Full signed triples
    Triples {
        triples: Vec<Triple>,
    },
    /// Ping to check liveness
    Ping,
    /// Pong response
    Pong,
    /// Error
    Error(String),
}

/// Lightweight triple header for the filtering step.
/// Contains enough information to decide whether to request the full triple,
/// without including the signature or full weight data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripleHeader {
    pub id: Uuid,
    /// Subject node value (resolved from NodeId)
    pub subject: String,
    /// Predicate value
    pub predicate: String,
    /// Object node value (resolved from NodeId)
    pub object: String,
    /// DID of the triple's author
    pub origin_did: Option<String>,
    /// When the triple was created
    pub timestamp: DateTime<Utc>,
    /// blake3 hash of the triple's content-addressable ID (for bloom filter)
    pub hash: [u8; 32],
}

/// Codec for serializing/deserializing gossip messages over the wire.
///
/// Wire format: 4-byte big-endian length prefix + JSON payload.
#[derive(Debug, Clone, Default)]
pub struct TripleSyncCodec;

#[async_trait]
impl request_response::Codec for TripleSyncCodec {
    type Protocol = StreamProtocol;
    type Request = GossipMessage;
    type Response = GossipMessage;

    async fn read_request<T: futures::AsyncRead + Unpin + Send>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request> {
        read_length_prefixed_json(io).await
    }

    async fn read_response<T: futures::AsyncRead + Unpin + Send>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response> {
        read_length_prefixed_json(io).await
    }

    async fn write_request<T: futures::AsyncWrite + Unpin + Send>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()> {
        write_length_prefixed_json(io, &req).await
    }

    async fn write_response<T: futures::AsyncWrite + Unpin + Send>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        resp: Self::Response,
    ) -> io::Result<()> {
        write_length_prefixed_json(io, &resp).await
    }
}

async fn read_length_prefixed_json<T: futures::AsyncRead + Unpin + Send, D: serde::de::DeserializeOwned>(
    io: &mut T,
) -> io::Result<D> {
    use futures::AsyncReadExt;

    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > MAX_MESSAGE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message too large: {} bytes (max {})", len, MAX_MESSAGE_SIZE),
        ));
    }

    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;

    serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

async fn write_length_prefixed_json<T: futures::AsyncWrite + Unpin + Send, S: serde::Serialize>(
    io: &mut T,
    value: &S,
) -> io::Result<()> {
    use futures::AsyncWriteExt;

    let buf = serde_json::to_vec(value)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    if buf.len() > MAX_MESSAGE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message too large: {} bytes (max {})", buf.len(), MAX_MESSAGE_SIZE),
        ));
    }

    let len = (buf.len() as u32).to_be_bytes();
    io.write_all(&len).await?;
    io.write_all(&buf).await?;
    io.flush().await?;

    Ok(())
}

/// Compute a 32-byte hash for bloom filter membership from a triple's content-addressable ID.
///
/// With content-addressable IDs, the TripleId already encodes (S, P, O) deterministically.
/// We use blake3 to expand the 16-byte TripleId into a 32-byte bloom filter key.
/// Same triple on any node produces the same hash.
pub fn triple_hash(triple: &Triple) -> [u8; 32] {
    *blake3::hash(triple.id.as_bytes()).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Node;

    #[test]
    fn test_gossip_message_bloom_exchange_serde() {
        let msg = GossipMessage::BloomExchange {
            filter: vec![0, 1, 2, 3],
            triple_count: 42,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: GossipMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            GossipMessage::BloomExchange { filter, triple_count } => {
                assert_eq!(filter, vec![0, 1, 2, 3]);
                assert_eq!(triple_count, 42);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_gossip_message_header_request_serde() {
        let hash = [0u8; 32];
        let msg = GossipMessage::HeaderRequest {
            triple_hashes: vec![hash],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: GossipMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            GossipMessage::HeaderRequest { triple_hashes } => {
                assert_eq!(triple_hashes.len(), 1);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_gossip_message_headers_serde() {
        let header = TripleHeader {
            id: Uuid::new_v4(),
            subject: "Alice".into(),
            predicate: "knows".into(),
            object: "Bob".into(),
            origin_did: Some("did:valence:key:abc".into()),
            timestamp: Utc::now(),
            hash: [0u8; 32],
        };
        let msg = GossipMessage::Headers {
            headers: vec![header],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: GossipMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            GossipMessage::Headers { headers } => {
                assert_eq!(headers.len(), 1);
                assert_eq!(headers[0].subject, "Alice");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_gossip_message_triples_serde() {
        let a = Node::new("A");
        let b = Node::new("B");
        let t = Triple::new(a.id, "knows", b.id);
        let msg = GossipMessage::Triples { triples: vec![t] };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: GossipMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            GossipMessage::Triples { triples } => {
                assert_eq!(triples.len(), 1);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_ping_pong_serde() {
        let msg = GossipMessage::Ping;
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, "\"Ping\"");

        let msg = GossipMessage::Pong;
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, "\"Pong\"");
    }

    #[test]
    fn test_error_serde() {
        let msg = GossipMessage::Error("test error".into());
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: GossipMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            GossipMessage::Error(e) => assert_eq!(e, "test error"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_triple_hash_deterministic() {
        let a = Node::new("A");
        let b = Node::new("B");
        let t = Triple::new(a.id, "knows", b.id);
        let h1 = triple_hash(&t);
        let h2 = triple_hash(&t);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_triple_hash_different_triples() {
        let a = Node::new("A");
        let b = Node::new("B");
        let c = Node::new("C");
        let t1 = Triple::new(a.id, "knows", b.id);
        let t2 = Triple::new(a.id, "knows", c.id);
        let h1 = triple_hash(&t1);
        let h2 = triple_hash(&t2);
        assert_ne!(h1, h2);
    }
}
