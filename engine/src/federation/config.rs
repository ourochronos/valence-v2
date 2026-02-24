//! Federation configuration.

use libp2p::Multiaddr;
use serde::{Deserialize, Serialize};

/// Trust thresholds for peer phase transitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustThresholds {
    /// Minimum trust score to become provisional (first successful interaction)
    pub provisional: f64,
    /// Minimum trust score to become established (multiple successful syncs)
    pub established: f64,
    /// Minimum trust score to become trusted (high trust + time)
    pub trusted: f64,
}

impl Default for TrustThresholds {
    fn default() -> Self {
        Self {
            provisional: 0.3,
            established: 0.6,
            trusted: 0.8,
        }
    }
}

/// Configuration for the federation subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    /// Address to listen on (default: /ip4/0.0.0.0/tcp/0 for random port)
    #[serde(
        serialize_with = "serialize_multiaddr",
        deserialize_with = "deserialize_multiaddr"
    )]
    pub listen_addr: Multiaddr,

    /// Bootstrap peers to connect to on startup
    #[serde(
        default,
        serialize_with = "serialize_multiaddr_vec",
        deserialize_with = "deserialize_multiaddr_vec"
    )]
    pub bootstrap_peers: Vec<Multiaddr>,

    /// Gossipsub topic for triple announcements
    #[serde(default = "default_gossipsub_topic")]
    pub gossipsub_topic: String,

    /// Interval between sync cycles in seconds
    #[serde(default = "default_sync_interval_secs")]
    pub sync_interval_secs: u64,

    /// Trust thresholds for peer phase transitions
    #[serde(default)]
    pub trust_thresholds: TrustThresholds,

    /// Decay factor for transitive trust (0.0-1.0, lower = faster decay per hop)
    #[serde(default = "default_trust_decay")]
    pub trust_decay: f64,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            listen_addr: "/ip4/0.0.0.0/tcp/0".parse().expect("valid default multiaddr"),
            bootstrap_peers: Vec::new(),
            gossipsub_topic: default_gossipsub_topic(),
            sync_interval_secs: default_sync_interval_secs(),
            trust_thresholds: TrustThresholds::default(),
            trust_decay: default_trust_decay(),
        }
    }
}

fn default_gossipsub_topic() -> String {
    "valence/triples/v1".to_string()
}

fn default_sync_interval_secs() -> u64 {
    300
}

fn default_trust_decay() -> f64 {
    0.7
}

// Custom serde for Multiaddr since it doesn't implement Serialize/Deserialize directly
fn serialize_multiaddr<S: serde::Serializer>(addr: &Multiaddr, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&addr.to_string())
}

fn deserialize_multiaddr<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Multiaddr, D::Error> {
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

    #[test]
    fn test_default_config() {
        let config = FederationConfig::default();
        assert_eq!(config.gossipsub_topic, "valence/triples/v1");
        assert_eq!(config.sync_interval_secs, 300);
        assert_eq!(config.trust_thresholds.provisional, 0.3);
        assert_eq!(config.trust_thresholds.established, 0.6);
        assert_eq!(config.trust_thresholds.trusted, 0.8);
        assert_eq!(config.trust_decay, 0.7);
    }

    #[test]
    fn test_trust_thresholds_ordering() {
        let t = TrustThresholds::default();
        assert!(t.provisional < t.established);
        assert!(t.established < t.trusted);
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let config = FederationConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: FederationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.gossipsub_topic, config.gossipsub_topic);
        assert_eq!(parsed.sync_interval_secs, config.sync_interval_secs);
    }
}
