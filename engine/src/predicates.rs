//! Well-known predicate constants for the Valence graph.
//! These are used as triple predicates with special semantics.

/// Identity/authorship
pub const AUTHORED: &str = "authored";
pub const SIGNED_BY: &str = "signed_by";

/// Trust (computed via PageRank of DID nodes)
pub const TRUSTS: &str = "trusts";

/// Sharing/privacy
pub const SHAREABLE_WITH: &str = "shareable_with";
pub const SHARE_POLICY: &str = "share_policy";
pub const LOCAL_ONLY: &str = "local_only";

/// Retraction (GDPR-compatible deletion)
pub const RETRACTED_BY: &str = "retracted_by";
pub const RETRACTED_AT: &str = "retracted_at";

/// Supersession (knowledge evolution)
pub const SUPERSEDES: &str = "supersedes";
pub const SUPERSEDE_REASON: &str = "supersede_reason";

/// Consent
pub const CONSENTS_TO: &str = "consents_to";

/// Verification
pub const VERIFIES: &str = "verifies";
pub const VERIFICATION_RESULT: &str = "verification_result";
pub const VERIFICATION_REASONING: &str = "verification_reasoning";

/// Tension resolution
pub const TENSION_RESOLVED_WITH: &str = "tension_resolved_with";
pub const TENSION_RESOLUTION_ACTION: &str = "tension_resolution_action";
pub const TENSION_RESOLUTION_REASONING: &str = "tension_resolution_reasoning";
