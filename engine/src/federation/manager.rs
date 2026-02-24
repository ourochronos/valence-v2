//! FederationManager: the main coordinator for P2P federation.
//!
//! Owns the transport, peer store, and graph view.
//! Handles the event loop, gossip cycles, and provides status methods.
//! Trust is computed via PageRank of DID nodes in the graph — no separate TrustManager.

use std::sync::Arc;

use anyhow::Result;
use libp2p::PeerId;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::graph::GraphView;
use crate::models::Triple;
use crate::storage::TripleStore;

use super::config::FederationConfig;
use super::peer::{InMemoryPeerStore, Peer, PeerStore, TrustPhase};
use super::protocol::GossipMessage;
use super::sync::BloomSync;
use super::transport::{FederationTransport, TransportEvent};

/// Status information about the federation subsystem.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FederationStatus {
    pub local_peer_id: String,
    pub connected_peers: usize,
    pub total_known_peers: usize,
    pub running: bool,
}

/// The main federation coordinator.
pub struct FederationManager {
    config: FederationConfig,
    transport: Arc<RwLock<FederationTransport>>,
    peer_store: Arc<dyn PeerStore>,
    store: Arc<dyn TripleStore>,
    local_peer_id: PeerId,
    running: Arc<RwLock<bool>>,
}

impl FederationManager {
    /// Create a new FederationManager from config and store.
    pub fn new(config: FederationConfig, store: Arc<dyn TripleStore>) -> Result<Self> {
        let transport = FederationTransport::new(&config)?;
        let local_peer_id = transport.local_peer_id();
        let peer_store: Arc<dyn PeerStore> = Arc::new(InMemoryPeerStore::new());

        Ok(Self {
            config,
            transport: Arc::new(RwLock::new(transport)),
            peer_store,
            store,
            local_peer_id,
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// Start the federation: listen, connect to bootstrap peers, start event loop.
    pub async fn start(&self) -> Result<()> {
        {
            let mut running = self.running.write().await;
            if *running {
                anyhow::bail!("federation already running");
            }
            *running = true;
        }

        // Start listening
        {
            let mut transport = self.transport.write().await;
            transport.listen(&self.config.listen_addr)?;

            // Connect to bootstrap peers
            for addr in &self.config.bootstrap_peers {
                if let Err(e) = transport.dial(addr) {
                    warn!("Failed to dial bootstrap peer {}: {}", addr, e);
                }
            }
        }

        info!(
            "Federation started. Local peer: {}",
            self.local_peer_id
        );

        Ok(())
    }

    /// Stop the federation gracefully.
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Federation stopped");
    }

    /// Run one iteration of the event loop. Call this in a loop from a tokio task.
    pub async fn handle_next_event(&self) -> Result<bool> {
        let running = *self.running.read().await;
        if !running {
            return Ok(false);
        }

        let event = {
            let mut transport = self.transport.write().await;
            transport.next_event().await
        };

        match event {
            Some(event) => {
                self.handle_event(event).await?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Handle a single transport event.
    async fn handle_event(&self, event: TransportEvent) -> Result<()> {
        match event {
            TransportEvent::PeerDiscovered { peer_id, addrs } => {
                info!("Peer discovered: {} at {:?}", peer_id, addrs);
                let mut peer = Peer::new(format!("pending:{}", peer_id), peer_id);
                peer.addrs = addrs;
                peer.connected = true;
                self.peer_store.add_peer(peer).await?;
            }

            TransportEvent::PeerDisconnected { peer_id } => {
                info!("Peer disconnected: {}", peer_id);
                let _ = self.peer_store.set_connected(&peer_id, false).await;
            }

            TransportEvent::GossipMessage { source, data: _ } => {
                debug!("Gossip announcement from {:?}", source);
                // Gossipsub announcements are simple notifications; the actual
                // sync happens via the request-response protocol in run_gossip_cycle.
            }

            TransportEvent::InboundRequest {
                peer_id,
                request,
                channel,
            } => {
                debug!("Inbound gossip request from {}", peer_id);
                let _ = self.peer_store.update_last_seen(&peer_id).await;

                let peer = self.peer_store.get_peer(&peer_id).await?;
                let peer_did = peer.map(|p| p.did.clone());

                let response = self.handle_gossip_request(request, peer_did.as_deref()).await?;
                let mut transport = self.transport.write().await;
                if let Err(e) = transport.send_response(channel, response) {
                    warn!("Failed to send response to {}: {}", peer_id, e);
                }
            }

            TransportEvent::InboundResponse {
                peer_id,
                response,
                ..
            } => {
                debug!("Inbound gossip response from {}", peer_id);
                let _ = self.peer_store.update_last_seen(&peer_id).await;
                self.handle_gossip_response(&peer_id, response).await?;
            }

            TransportEvent::OutboundFailure {
                peer_id, error, ..
            } => {
                warn!("Outbound failure to {}: {}", peer_id, error);
            }
        }

        Ok(())
    }

    /// Handle an inbound gossip protocol request and produce a response.
    async fn handle_gossip_request(
        &self,
        request: GossipMessage,
        recipient_did: Option<&str>,
    ) -> Result<GossipMessage> {
        match request {
            GossipMessage::BloomExchange { filter: _, triple_count: _ } => {
                // Peer sent their bloom filter. Respond with ours so they can
                // determine what we're missing. The requester handles the
                // comparison in handle_gossip_response.
                let (our_filter, our_count) =
                    BloomSync::build_bloom_filter(self.store.as_ref(), recipient_did).await?;
                let our_filter_bytes = our_filter.bitmap();
                Ok(GossipMessage::BloomExchange {
                    filter: our_filter_bytes,
                    triple_count: our_count,
                })
            }

            GossipMessage::HeaderRequest { triple_hashes } => {
                let headers =
                    BloomSync::build_headers_for_hashes(self.store.as_ref(), &triple_hashes, recipient_did).await?;
                Ok(GossipMessage::Headers { headers })
            }

            GossipMessage::TripleRequest { triple_ids } => {
                let triples =
                    BloomSync::collect_triples_by_ids(self.store.as_ref(), &triple_ids, recipient_did).await?;
                Ok(GossipMessage::Triples { triples })
            }

            GossipMessage::Ping => Ok(GossipMessage::Pong),

            other => {
                warn!("Unexpected inbound request type: {:?}", other);
                Ok(GossipMessage::Error("unexpected request type".into()))
            }
        }
    }

    /// Handle an inbound gossip response (from a request we sent).
    async fn handle_gossip_response(
        &self,
        peer_id: &PeerId,
        response: GossipMessage,
    ) -> Result<()> {
        match response {
            GossipMessage::Headers { headers } => {
                // We received headers from a peer. Filter and request full triples.
                let graph = GraphView::from_store(self.store.as_ref()).await?;
                let wanted = BloomSync::filter_wanted_headers(&headers, self.store.as_ref(), &graph).await;

                if !wanted.is_empty() {
                    debug!("Requesting {} triples from {}", wanted.len(), peer_id);
                    let mut transport = self.transport.write().await;
                    transport.send_request(peer_id, GossipMessage::TripleRequest { triple_ids: wanted });
                }
            }

            GossipMessage::Triples { triples } => {
                // We received full triples. Merge them (CRDT: insert new, corroborate existing).
                let graph = GraphView::from_store(self.store.as_ref()).await?;
                let result = BloomSync::process_received_triples(triples, self.store.as_ref(), &graph).await;

                let inserted_count = result.inserted.len();
                let corroborated_count = result.corroborated.len();

                for triple in result.inserted {
                    if let Err(e) = self.store.insert_triple(triple).await {
                        warn!("Failed to insert received triple: {}", e);
                    }
                }

                if inserted_count > 0 || corroborated_count > 0 {
                    info!(
                        "Merged triples from {}: {} inserted, {} corroborated",
                        peer_id, inserted_count, corroborated_count
                    );
                    let _ = self.peer_store.increment_successful_syncs(peer_id).await;
                }
            }

            GossipMessage::BloomExchange { filter, triple_count } => {
                // We received peer's bloom filter response. Find what they're missing
                // and send headers.
                let peer = self.peer_store.get_peer(peer_id).await?;
                let peer_did = peer.map(|p| p.did.clone());

                let missing = BloomSync::find_missing_for_peer(
                    self.store.as_ref(),
                    &filter,
                    triple_count,
                    peer_did.as_deref(),
                ).await?;

                if !missing.is_empty() {
                    debug!("Peer {} is missing {} triples, sending header request", peer_id, missing.len());
                    let mut transport = self.transport.write().await;
                    transport.send_request(peer_id, GossipMessage::HeaderRequest { triple_hashes: missing });
                }
            }

            GossipMessage::Pong => {
                debug!("Pong from {}", peer_id);
            }

            GossipMessage::Error(msg) => {
                warn!("Error from {}: {}", peer_id, msg);
            }

            other => {
                warn!("Unexpected response type from {}: {:?}", peer_id, other);
            }
        }

        Ok(())
    }

    /// Run a gossip cycle: for each connected peer, exchange bloom filters to
    /// discover and sync missing triples.
    pub async fn run_gossip_cycle(&self) -> Result<usize> {
        let peers = self.peers_to_sync().await?;
        let count = peers.len();
        debug!("Running gossip cycle with {} peers", count);

        for peer_id in &peers {
            // Get peer's DID for privacy filtering
            let peer = self.peer_store.get_peer(peer_id).await?;
            let peer_did = peer.map(|p| p.did.clone());

            // Build our bloom filter (with privacy filtering for this peer)
            let (filter, triple_count) =
                BloomSync::build_bloom_filter(self.store.as_ref(), peer_did.as_deref()).await?;
            let filter_bytes = filter.bitmap();

            // Send our bloom filter to the peer
            let mut transport = self.transport.write().await;
            transport.send_request(
                peer_id,
                GossipMessage::BloomExchange {
                    filter: filter_bytes,
                    triple_count,
                },
            );
        }

        Ok(count)
    }

    /// Get the list of peers eligible for gossip sync.
    async fn peers_to_sync(&self) -> Result<Vec<PeerId>> {
        let peers = self.peer_store.list_peers().await?;
        Ok(peers
            .into_iter()
            .filter(|p| {
                p.connected && p.trust_phase.level() >= TrustPhase::Provisional.level()
            })
            .map(|p| p.peer_id)
            .collect())
    }

    /// Broadcast a triple announcement to all peers via gossipsub.
    pub async fn broadcast_triple(&self, triple: &Triple) -> Result<()> {
        let announcement = serde_json::to_vec(&serde_json::json!({
            "triple_id": triple.id,
            "origin_did": triple.origin_did,
            "timestamp": triple.timestamp,
        }))?;
        let mut transport = self.transport.write().await;
        transport.gossip_publish(announcement)?;
        debug!("Broadcast triple announcement: {}", triple.id);
        Ok(())
    }

    /// Get current federation status.
    pub async fn status(&self) -> Result<FederationStatus> {
        let peers = self.peer_store.list_peers().await?;
        let connected = peers.iter().filter(|p| p.connected).count();
        let running = *self.running.read().await;

        Ok(FederationStatus {
            local_peer_id: self.local_peer_id.to_string(),
            connected_peers: connected,
            total_known_peers: peers.len(),
            running,
        })
    }

    /// Get the local peer ID.
    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    /// Get a reference to the peer store.
    pub fn peer_store(&self) -> &Arc<dyn PeerStore> {
        &self.peer_store
    }

    /// Get a reference to the triple store.
    pub fn store(&self) -> &Arc<dyn TripleStore> {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryStore;

    #[test]
    fn test_federation_manager_creation() {
        let config = FederationConfig::default();
        let store: Arc<dyn TripleStore> = Arc::new(MemoryStore::new());
        let fm = FederationManager::new(config, store);
        assert!(fm.is_ok());
    }

    #[tokio::test]
    async fn test_federation_status_initial() {
        let config = FederationConfig::default();
        let store: Arc<dyn TripleStore> = Arc::new(MemoryStore::new());
        let fm = FederationManager::new(config, store).unwrap();

        let status = fm.status().await.unwrap();
        assert_eq!(status.connected_peers, 0);
        assert_eq!(status.total_known_peers, 0);
        assert!(!status.running);
    }

    #[tokio::test]
    async fn test_federation_stop_without_start() {
        let config = FederationConfig::default();
        let store: Arc<dyn TripleStore> = Arc::new(MemoryStore::new());
        let fm = FederationManager::new(config, store).unwrap();
        fm.stop().await; // Should not panic
    }

    #[tokio::test]
    async fn test_handle_gossip_request_ping() {
        let config = FederationConfig::default();
        let store: Arc<dyn TripleStore> = Arc::new(MemoryStore::new());
        let fm = FederationManager::new(config, store).unwrap();

        let response = fm.handle_gossip_request(GossipMessage::Ping, None).await.unwrap();
        matches!(response, GossipMessage::Pong);
    }

    #[tokio::test]
    async fn test_handle_gossip_request_triple_request() {
        let config = FederationConfig::default();
        let store = Arc::new(MemoryStore::new());

        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let t = Triple::new(a.id, "knows", b.id);
        let tid = t.id;
        store.insert_triple(t).await.unwrap();

        let fm = FederationManager::new(config, store.clone() as Arc<dyn TripleStore>).unwrap();

        let response = fm
            .handle_gossip_request(GossipMessage::TripleRequest { triple_ids: vec![tid] }, None)
            .await
            .unwrap();

        match response {
            GossipMessage::Triples { triples } => {
                assert_eq!(triples.len(), 1);
                assert_eq!(triples[0].id, tid);
            }
            _ => panic!("wrong response type"),
        }
    }
}
