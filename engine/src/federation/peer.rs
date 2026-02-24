//! Peer management: tracking connected peers, their trust phase, and capabilities.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use libp2p::PeerId;
use libp2p::Multiaddr;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Trust phase of a peer — determines what operations are allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustPhase {
    /// No prior interaction
    Unknown,
    /// First interaction completed, basic sync allowed
    Provisional,
    /// Multiple successful syncs, broader sync allowed
    Established,
    /// High trust score sustained over time, full sharing
    Trusted,
}

impl TrustPhase {
    /// Numeric ordering for comparison
    pub fn level(&self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Provisional => 1,
            Self::Established => 2,
            Self::Trusted => 3,
        }
    }
}

/// A known peer in the federation network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    /// The peer's DID (valence identity)
    pub did: String,
    /// The libp2p PeerId
    #[serde(
        serialize_with = "serialize_peer_id",
        deserialize_with = "deserialize_peer_id"
    )]
    pub peer_id: PeerId,
    /// Known addresses for this peer
    #[serde(
        serialize_with = "serialize_multiaddr_vec",
        deserialize_with = "deserialize_multiaddr_vec"
    )]
    pub addrs: Vec<Multiaddr>,
    /// Capabilities advertised by this peer
    pub capabilities: Vec<String>,
    /// Current trust phase
    pub trust_phase: TrustPhase,
    /// Protocol version string
    pub protocol_version: String,
    /// Last time we heard from this peer
    pub last_seen: DateTime<Utc>,
    /// Whether the peer is currently connected
    pub connected: bool,
    /// Number of successful sync cycles with this peer
    pub successful_syncs: u64,
}

impl Peer {
    pub fn new(did: String, peer_id: PeerId) -> Self {
        Self {
            did,
            peer_id,
            addrs: Vec::new(),
            capabilities: Vec::new(),
            trust_phase: TrustPhase::Unknown,
            protocol_version: "valence/1.0".to_string(),
            last_seen: Utc::now(),
            connected: false,
            successful_syncs: 0,
        }
    }
}

/// Storage trait for peer information.
#[async_trait]
pub trait PeerStore: Send + Sync {
    async fn add_peer(&self, peer: Peer) -> anyhow::Result<()>;
    async fn get_peer(&self, peer_id: &PeerId) -> anyhow::Result<Option<Peer>>;
    async fn get_peer_by_did(&self, did: &str) -> anyhow::Result<Option<Peer>>;
    async fn list_peers(&self) -> anyhow::Result<Vec<Peer>>;
    async fn update_trust_phase(&self, peer_id: &PeerId, phase: TrustPhase) -> anyhow::Result<()>;
    async fn update_last_seen(&self, peer_id: &PeerId) -> anyhow::Result<()>;
    async fn set_connected(&self, peer_id: &PeerId, connected: bool) -> anyhow::Result<()>;
    async fn increment_successful_syncs(&self, peer_id: &PeerId) -> anyhow::Result<()>;
    async fn remove_peer(&self, peer_id: &PeerId) -> anyhow::Result<()>;
}

/// In-memory implementation of PeerStore.
#[derive(Clone)]
pub struct InMemoryPeerStore {
    peers: Arc<RwLock<HashMap<PeerId, Peer>>>,
}

impl InMemoryPeerStore {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryPeerStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PeerStore for InMemoryPeerStore {
    async fn add_peer(&self, peer: Peer) -> anyhow::Result<()> {
        let mut peers = self.peers.write().await;
        peers.insert(peer.peer_id, peer);
        Ok(())
    }

    async fn get_peer(&self, peer_id: &PeerId) -> anyhow::Result<Option<Peer>> {
        let peers = self.peers.read().await;
        Ok(peers.get(peer_id).cloned())
    }

    async fn get_peer_by_did(&self, did: &str) -> anyhow::Result<Option<Peer>> {
        let peers = self.peers.read().await;
        Ok(peers.values().find(|p| p.did == did).cloned())
    }

    async fn list_peers(&self) -> anyhow::Result<Vec<Peer>> {
        let peers = self.peers.read().await;
        Ok(peers.values().cloned().collect())
    }

    async fn update_trust_phase(&self, peer_id: &PeerId, phase: TrustPhase) -> anyhow::Result<()> {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(peer_id) {
            peer.trust_phase = phase;
            Ok(())
        } else {
            anyhow::bail!("peer not found: {}", peer_id)
        }
    }

    async fn update_last_seen(&self, peer_id: &PeerId) -> anyhow::Result<()> {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(peer_id) {
            peer.last_seen = Utc::now();
            Ok(())
        } else {
            anyhow::bail!("peer not found: {}", peer_id)
        }
    }

    async fn set_connected(&self, peer_id: &PeerId, connected: bool) -> anyhow::Result<()> {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(peer_id) {
            peer.connected = connected;
            if connected {
                peer.last_seen = Utc::now();
            }
            Ok(())
        } else {
            anyhow::bail!("peer not found: {}", peer_id)
        }
    }

    async fn increment_successful_syncs(&self, peer_id: &PeerId) -> anyhow::Result<()> {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(peer_id) {
            peer.successful_syncs += 1;
            Ok(())
        } else {
            anyhow::bail!("peer not found: {}", peer_id)
        }
    }

    async fn remove_peer(&self, peer_id: &PeerId) -> anyhow::Result<()> {
        let mut peers = self.peers.write().await;
        peers.remove(peer_id);
        Ok(())
    }
}

// Serde helpers for PeerId
fn serialize_peer_id<S: serde::Serializer>(peer_id: &PeerId, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&peer_id.to_string())
}

fn deserialize_peer_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<PeerId, D::Error> {
    let s = String::deserialize(d)?;
    s.parse().map_err(serde::de::Error::custom)
}

fn serialize_multiaddr_vec<S: serde::Serializer>(
    addrs: &[Multiaddr],
    s: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let mut seq = s.serialize_seq(Some(addrs.len()))?;
    for addr in addrs {
        seq.serialize_element(&addr.to_string())?;
    }
    seq.end()
}

fn deserialize_multiaddr_vec<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<Multiaddr>, D::Error> {
    let strings: Vec<String> = Vec::deserialize(d)?;
    strings
        .into_iter()
        .map(|s| s.parse().map_err(serde::de::Error::custom))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair as Libp2pKeypair;

    fn test_peer_id() -> PeerId {
        let kp = Libp2pKeypair::generate_ed25519();
        PeerId::from(kp.public())
    }

    #[tokio::test]
    async fn test_add_and_get_peer() {
        let store = InMemoryPeerStore::new();
        let pid = test_peer_id();
        let peer = Peer::new("did:valence:key:abc".into(), pid);

        store.add_peer(peer.clone()).await.unwrap();
        let fetched = store.get_peer(&pid).await.unwrap().unwrap();
        assert_eq!(fetched.did, "did:valence:key:abc");
        assert_eq!(fetched.trust_phase, TrustPhase::Unknown);
    }

    #[tokio::test]
    async fn test_get_peer_by_did() {
        let store = InMemoryPeerStore::new();
        let pid = test_peer_id();
        let peer = Peer::new("did:valence:key:xyz".into(), pid);

        store.add_peer(peer).await.unwrap();
        let fetched = store.get_peer_by_did("did:valence:key:xyz").await.unwrap();
        assert!(fetched.is_some());
        let fetched = store.get_peer_by_did("did:valence:key:nope").await.unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn test_list_peers() {
        let store = InMemoryPeerStore::new();
        let p1 = test_peer_id();
        let p2 = test_peer_id();

        store.add_peer(Peer::new("did:1".into(), p1)).await.unwrap();
        store.add_peer(Peer::new("did:2".into(), p2)).await.unwrap();

        let peers = store.list_peers().await.unwrap();
        assert_eq!(peers.len(), 2);
    }

    #[tokio::test]
    async fn test_trust_phase_update() {
        let store = InMemoryPeerStore::new();
        let pid = test_peer_id();
        store.add_peer(Peer::new("did:test".into(), pid)).await.unwrap();

        store.update_trust_phase(&pid, TrustPhase::Provisional).await.unwrap();
        let peer = store.get_peer(&pid).await.unwrap().unwrap();
        assert_eq!(peer.trust_phase, TrustPhase::Provisional);

        store.update_trust_phase(&pid, TrustPhase::Established).await.unwrap();
        let peer = store.get_peer(&pid).await.unwrap().unwrap();
        assert_eq!(peer.trust_phase, TrustPhase::Established);

        store.update_trust_phase(&pid, TrustPhase::Trusted).await.unwrap();
        let peer = store.get_peer(&pid).await.unwrap().unwrap();
        assert_eq!(peer.trust_phase, TrustPhase::Trusted);
    }

    #[tokio::test]
    async fn test_connected_state() {
        let store = InMemoryPeerStore::new();
        let pid = test_peer_id();
        store.add_peer(Peer::new("did:conn".into(), pid)).await.unwrap();

        assert!(!store.get_peer(&pid).await.unwrap().unwrap().connected);
        store.set_connected(&pid, true).await.unwrap();
        assert!(store.get_peer(&pid).await.unwrap().unwrap().connected);
        store.set_connected(&pid, false).await.unwrap();
        assert!(!store.get_peer(&pid).await.unwrap().unwrap().connected);
    }

    #[tokio::test]
    async fn test_increment_syncs() {
        let store = InMemoryPeerStore::new();
        let pid = test_peer_id();
        store.add_peer(Peer::new("did:sync".into(), pid)).await.unwrap();

        store.increment_successful_syncs(&pid).await.unwrap();
        store.increment_successful_syncs(&pid).await.unwrap();
        let peer = store.get_peer(&pid).await.unwrap().unwrap();
        assert_eq!(peer.successful_syncs, 2);
    }

    #[tokio::test]
    async fn test_remove_peer() {
        let store = InMemoryPeerStore::new();
        let pid = test_peer_id();
        store.add_peer(Peer::new("did:rm".into(), pid)).await.unwrap();

        store.remove_peer(&pid).await.unwrap();
        assert!(store.get_peer(&pid).await.unwrap().is_none());
    }

    #[test]
    fn test_trust_phase_ordering() {
        assert!(TrustPhase::Unknown.level() < TrustPhase::Provisional.level());
        assert!(TrustPhase::Provisional.level() < TrustPhase::Established.level());
        assert!(TrustPhase::Established.level() < TrustPhase::Trusted.level());
    }
}
