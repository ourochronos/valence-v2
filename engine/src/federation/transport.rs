//! libp2p swarm setup and transport layer.
//!
//! Composes Gossipsub + Kademlia + mDNS + Identify + RequestResponse into a
//! single swarm behavior, and provides a `FederationTransport` that owns the
//! swarm and exposes send/receive methods.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use anyhow::{Context, Result};
use futures::StreamExt;
use libp2p::{
    gossipsub, identify, kad, mdns, noise,
    request_response::{self, ProtocolSupport},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::config::FederationConfig;
use super::protocol::{GossipMessage, TripleSyncCodec, PROTOCOL_NAME};

/// Composed network behaviour for the Valence federation.
#[derive(NetworkBehaviour)]
pub struct ValenceBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    pub mdns: mdns::tokio::Behaviour,
    pub identify: identify::Behaviour,
    pub request_response: request_response::Behaviour<TripleSyncCodec>,
}

/// Events produced by the transport layer for the federation manager to handle.
#[derive(Debug)]
pub enum TransportEvent {
    /// A new peer was discovered (via mDNS or Kademlia)
    PeerDiscovered {
        peer_id: PeerId,
        addrs: Vec<Multiaddr>,
    },
    /// A peer disconnected
    PeerDisconnected {
        peer_id: PeerId,
    },
    /// Received a gossipsub message (triple announcement)
    GossipMessage {
        source: Option<PeerId>,
        data: Vec<u8>,
    },
    /// Received a gossip protocol request from a peer
    InboundRequest {
        peer_id: PeerId,
        request: super::protocol::GossipMessage,
        channel: request_response::ResponseChannel<super::protocol::GossipMessage>,
    },
    /// Received a gossip protocol response from a peer
    InboundResponse {
        peer_id: PeerId,
        request_id: request_response::OutboundRequestId,
        response: super::protocol::GossipMessage,
    },
    /// A request we sent failed
    OutboundFailure {
        peer_id: PeerId,
        request_id: request_response::OutboundRequestId,
        error: String,
    },
}

/// The federation transport layer. Owns the libp2p swarm and provides
/// methods to send messages and receive events.
pub struct FederationTransport {
    swarm: Swarm<ValenceBehaviour>,
    event_tx: mpsc::Sender<TransportEvent>,
    event_rx: mpsc::Receiver<TransportEvent>,
    gossip_topic: gossipsub::IdentTopic,
    local_peer_id: PeerId,
}

impl FederationTransport {
    /// Create a new transport from the given configuration.
    pub fn new(config: &FederationConfig) -> Result<Self> {
        let swarm = build_swarm(config)?;
        let local_peer_id = *swarm.local_peer_id();
        let gossip_topic = gossipsub::IdentTopic::new(&config.gossipsub_topic);
        let (event_tx, event_rx) = mpsc::channel(256);

        Ok(Self {
            swarm,
            event_tx,
            event_rx,
            gossip_topic,
            local_peer_id,
        })
    }

    /// Get the local PeerId.
    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    /// Start listening on the configured address.
    pub fn listen(&mut self, addr: &Multiaddr) -> Result<()> {
        self.swarm
            .listen_on(addr.clone())
            .context("failed to start listening")?;
        info!("Listening on {}", addr);
        Ok(())
    }

    /// Dial a peer at the given address.
    pub fn dial(&mut self, addr: &Multiaddr) -> Result<()> {
        self.swarm
            .dial(addr.clone())
            .context("failed to dial peer")?;
        debug!("Dialing {}", addr);
        Ok(())
    }

    /// Publish a message to the gossipsub topic (triple announcement).
    pub fn gossip_publish(&mut self, data: Vec<u8>) -> Result<()> {
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(self.gossip_topic.clone(), data)
            .map_err(|e| anyhow::anyhow!("gossipsub publish failed: {}", e))?;
        Ok(())
    }

    /// Send a gossip protocol request to a specific peer.
    pub fn send_request(&mut self, peer_id: &PeerId, request: GossipMessage) -> request_response::OutboundRequestId {
        self.swarm
            .behaviour_mut()
            .request_response
            .send_request(peer_id, request)
    }

    /// Send a gossip protocol response back on the given channel.
    pub fn send_response(
        &mut self,
        channel: request_response::ResponseChannel<GossipMessage>,
        response: GossipMessage,
    ) -> Result<()> {
        self.swarm
            .behaviour_mut()
            .request_response
            .send_response(channel, response)
            .map_err(|_| anyhow::anyhow!("failed to send response (channel closed)"))
    }

    /// Poll the next transport event. Returns None when the event loop should stop.
    pub async fn next_event(&mut self) -> Option<TransportEvent> {
        // First check if there are buffered events
        if let Ok(event) = self.event_rx.try_recv() {
            return Some(event);
        }

        // Otherwise poll the swarm
        loop {
            let event = self.swarm.next().await;
            match event {
                Some(swarm_event) => {
                    if let Some(transport_event) = self.handle_swarm_event(swarm_event).await {
                        return Some(transport_event);
                    }
                    // If handle_swarm_event returned None, the event was handled internally
                    // (e.g. kademlia routing table update). Loop to get next event.
                }
                None => return None,
            }
        }
    }

    /// Process a raw swarm event into a transport event.
    async fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<ValenceBehaviourEvent>,
    ) -> Option<TransportEvent> {
        match event {
            // mDNS discovery
            SwarmEvent::Behaviour(ValenceBehaviourEvent::Mdns(mdns::Event::Discovered(peers))) => {
                for (peer_id, addr) in peers {
                    debug!("mDNS discovered peer {} at {}", peer_id, addr);
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .add_explicit_peer(&peer_id);
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, addr.clone());
                    return Some(TransportEvent::PeerDiscovered {
                        peer_id,
                        addrs: vec![addr],
                    });
                }
                None
            }

            SwarmEvent::Behaviour(ValenceBehaviourEvent::Mdns(mdns::Event::Expired(peers))) => {
                for (peer_id, _addr) in peers {
                    debug!("mDNS peer expired: {}", peer_id);
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .remove_explicit_peer(&peer_id);
                }
                None
            }

            // Gossipsub messages
            SwarmEvent::Behaviour(ValenceBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source,
                message,
                ..
            })) => {
                debug!("Gossipsub message from {:?}", propagation_source);
                Some(TransportEvent::GossipMessage {
                    source: message.source,
                    data: message.data,
                })
            }

            // Request-response inbound
            SwarmEvent::Behaviour(ValenceBehaviourEvent::RequestResponse(
                request_response::Event::Message {
                    peer,
                    message:
                        request_response::Message::Request {
                            request, channel, ..
                        },
                    ..
                },
            )) => {
                debug!("Inbound gossip request from {}", peer);
                Some(TransportEvent::InboundRequest {
                    peer_id: peer,
                    request,
                    channel,
                })
            }

            // Request-response response received
            SwarmEvent::Behaviour(ValenceBehaviourEvent::RequestResponse(
                request_response::Event::Message {
                    peer,
                    message:
                        request_response::Message::Response {
                            request_id,
                            response,
                        },
                    ..
                },
            )) => {
                debug!("Inbound gossip response from {}", peer);
                Some(TransportEvent::InboundResponse {
                    peer_id: peer,
                    request_id,
                    response,
                })
            }

            // Request failed
            SwarmEvent::Behaviour(ValenceBehaviourEvent::RequestResponse(
                request_response::Event::OutboundFailure {
                    peer,
                    request_id,
                    error,
                    ..
                },
            )) => {
                warn!("Outbound request to {} failed: {:?}", peer, error);
                Some(TransportEvent::OutboundFailure {
                    peer_id: peer,
                    request_id,
                    error: format!("{:?}", error),
                })
            }

            // Connection established
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                info!("Connection established with {}", peer_id);
                None
            }

            // Connection closed
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                info!("Connection closed with {}", peer_id);
                Some(TransportEvent::PeerDisconnected { peer_id })
            }

            // Listening on address
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on {}", address);
                None
            }

            // Other events we don't need to surface
            _ => None,
        }
    }
}

/// Build a configured libp2p swarm.
fn build_swarm(config: &FederationConfig) -> Result<Swarm<ValenceBehaviour>> {
    let swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .map_err(|e| anyhow::anyhow!("TCP transport error: {}", e))?
        .with_dns()
        .map_err(|e| anyhow::anyhow!("DNS transport error: {}", e))?
        .with_behaviour(|key| {
            // Gossipsub
            let message_id_fn = |message: &gossipsub::Message| {
                let mut hasher = DefaultHasher::new();
                message.data.hash(&mut hasher);
                if let Some(source) = &message.source {
                    source.to_bytes().hash(&mut hasher);
                }
                gossipsub::MessageId::from(hasher.finish().to_string())
            };

            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(10))
                .validation_mode(gossipsub::ValidationMode::Strict)
                .message_id_fn(message_id_fn)
                .build()
                .expect("valid gossipsub config");

            let mut gossipsub =
                gossipsub::Behaviour::new(gossipsub::MessageAuthenticity::Signed(key.clone()), gossipsub_config)
                    .expect("valid gossipsub behaviour");

            let topic = gossipsub::IdentTopic::new(&config.gossipsub_topic);
            gossipsub.subscribe(&topic).expect("subscribe to topic");

            // Kademlia
            let peer_id = PeerId::from(key.public());
            let kademlia = kad::Behaviour::new(peer_id, kad::store::MemoryStore::new(peer_id));

            // mDNS
            let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
                .expect("valid mDNS config");

            // Identify
            let identify = identify::Behaviour::new(identify::Config::new(
                "/valence/2.0.0".to_string(),
                key.public(),
            ));

            // Request-Response for gossip sync
            let request_response = request_response::Behaviour::new(
                [(PROTOCOL_NAME, ProtocolSupport::Full)],
                request_response::Config::default(),
            );

            Ok(ValenceBehaviour {
                gossipsub,
                kademlia,
                mdns,
                identify,
                request_response,
            })
        })
        .map_err(|e| anyhow::anyhow!("Behaviour error: {}", e))?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    Ok(swarm)
}

// Note: transport tests that require actual networking are integration tests.
// Unit tests for the transport are limited to construction and configuration.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_creation() {
        let config = FederationConfig::default();
        let transport = FederationTransport::new(&config);
        assert!(transport.is_ok());
    }

    #[test]
    fn test_local_peer_id() {
        let config = FederationConfig::default();
        let transport = FederationTransport::new(&config).unwrap();
        // PeerId should be valid and non-zero length
        let pid = transport.local_peer_id();
        assert!(!pid.to_string().is_empty());
    }
}
