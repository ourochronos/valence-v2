//! P2P federation via libp2p.
//!
//! This module provides peer-to-peer knowledge sharing between Valence engines
//! using gossipsub for announcements, Kademlia for peer discovery, mDNS for
//! local discovery, and bloom filter gossip for triple synchronization.
//!
//! Trust is computed via PageRank of DID nodes in the graph (see graph::algorithms).
//! Privacy filtering uses well-known predicates (see predicates.rs).

pub mod config;
pub mod peer;
pub mod protocol;
pub mod transport;
pub mod sync;
pub mod manager;

pub use config::{FederationConfig, TrustThresholds};
pub use peer::{Peer, TrustPhase, PeerStore, InMemoryPeerStore};
pub use protocol::{GossipMessage, TripleHeader};
pub use sync::{BloomSync, MergeResult};
pub use manager::FederationManager;
