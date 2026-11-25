//! The container manifest: the artifact's self-description (its "DNA"),
//! serialized as canonical CBOR.
//!
//! The struct field names, order and serde attributes match
//! `caf-core/manifest.rs` verbatim to keep byte-level compatibility with the
//! CAF tooling. The manifest carries no absolute offsets (the layout lives in
//! the binary directory), so its content hash is layout-independent.

use serde::{Deserialize, Serialize};

/// Payload files are chunked at a fixed size and hashed as a Merkle tree. A
/// file at or below one chunk hashes directly to the same digest as the whole
/// file.
pub const CHUNK_SIZE: usize = 1024 * 1024;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// The payload tree node kind.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    /// A regular file: a leaf node with a content hash.
    File,
    /// A directory: its hash covers the child nodes.
    Dir,
}

impl EntryKind {
    /// The lowercase wire name used for serialization.
    pub fn as_str(self) -> &'static str {
        match self {
            EntryKind::File => "file",
            EntryKind::Dir => "dir",
        }
    }
}

/// The access mode of a component's payload access scope.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum AccessMode {
    /// Read-only (the only value in format v1).
    Read,
}

/// A read-layout hint for payload file bytes.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum PayloadLayout {
    /// Read as one block (single chunk).
    Opaque,
    /// Chunked / range-readable (multiple chunks).
    Range,
    /// Streaming (reserved).
    Stream,
}

/// Payload compression (reserved; not implemented in v1).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    /// Zstandard.
    Zstd,
    /// Gzip.
    Gzip,
}

// ---------------------------------------------------------------------------
// Capability id
// ---------------------------------------------------------------------------

/// `namespace:name@version`, e.g. `cf:spatial-query@1`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CapabilityId {
    /// The namespace prefix.
    pub namespace: String,
    /// The interface name.
    pub name: String,
    /// The interface version (the `@N` suffix).
    pub version: u32,
}

impl CapabilityId {
    /// Parses `namespace:name@version`.
    pub fn parse(s: &str) -> Result<Self, String> {
        let (name_ns, version) = s
            .rsplit_once('@')
            .ok_or_else(|| format!("capability id '{s}' is missing the '@version' suffix"))?;
        let version: u32 = version
            .parse()
            .map_err(|_| format!("capability id '{s}' has a non-numeric version"))?;
        let (namespace, name) = name_ns
            .rsplit_once(':')
            .ok_or_else(|| format!("capability id '{s}' is missing the ':' separator"))?;
        if namespace.is_empty() || name.is_empty() {
            return Err(format!(
                "capability id '{s}' has an empty namespace or name"
            ));
        }
        Ok(Self {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version,
        })
    }
}

impl std::fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}@{}", self.namespace, self.name, self.version)
    }
}

impl Serialize for CapabilityId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CapabilityId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        CapabilityId::parse(&s).map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

/// The artifact's self-description: identity, interface, components,
/// capabilities, execution profile, resource limits, policy, provenance,
/// payload trees, and the hashes of the other sections.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// The artifact name.
    pub name: String,
    /// The artifact version (semver).
    pub version: String,
    /// The container format version (must match the header).
    pub format_version: u32,
    /// Reference to the interface (WIT text) section.
    pub interface: InterfaceRef,
    /// The embedded WASM components.
    pub components: Vec<ComponentRef>,
    /// The capabilities the artifact exposes (`namespace:name@version`).
    pub capabilities: Vec<CapabilityId>,
    /// The execution profile the components run under.
    pub execution: ExecutionProfile,
    /// The declared resource budget.
    pub resources: ResourceLimits,
    /// The capability policy.
    pub policy: CapabilityPolicy,
    /// The publisher provenance.
    pub provenance: Provenance,
    /// The payload trees (the first is the default payload, mounted at `/`).
    pub payloads: Vec<PayloadRef>,
    /// Content hashes of the other sections (interface / wasm / payload).
    pub hashes: SectionHashes,
}

/// A reference to the interface (WIT text) section.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct InterfaceRef {
    /// Whether the WIT text is inlined in the manifest (unused in format v1).
    pub inline: bool,
    /// Byte length of the interface section.
    pub length: u64,
    /// SHA-256 of the interface section (hex).
    pub hash: String,
}

/// The manifest binding of one embedded WASM component.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ComponentRef {
    /// The component name (unique within the artifact).
    pub name: String,
    /// The WIT world the component implements (empty when no interface is declared).
    pub world: String,
    /// Byte length of the component section.
    pub length: u64,
    /// SHA-256 of the component bytes (hex).
    pub hash: String,
    /// The exports bound to capabilities.
    pub exports: Vec<ExportEntry>,
    /// The payload paths the component may read (a hard runtime invariant).
    pub access: Vec<AccessScope>,
}

/// One exported function, bound to its capability.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ExportEntry {
    /// The owning capability id (`namespace:name@version`).
    pub capability: String,
    /// The export name.
    pub export: String,
    /// The function signature (WIT-derived, or the raw WASM export type).
    pub signature: String,
}

/// A payload path the component may read, bound to the node's hash.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct AccessScope {
    /// The payload name (empty means the default payload).
    pub payload: String,
    /// The payload path, starting with `/`.
    pub path: String,
    /// SHA-256 of the node at that path (hex).
    pub hash: String,
    /// The access mode (read-only in format v1).
    pub mode: AccessMode,
}

/// The machine-checkable engine configuration the components run under.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ExecutionProfile {
    /// The profile name.
    pub profile: String,
    /// Whether execution must be bit-for-bit deterministic.
    pub deterministic: bool,
}

/// The declared resource budget.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ResourceLimits {
    /// Maximum guest linear memory (bytes).
    pub memory_max: u64,
    /// CPU budget (in fuel units).
    pub cpu_fuel: u64,
    /// Maximum bytes read from the payload backing.
    pub read_max: u64,
    /// Maximum output bytes.
    pub output_max: u64,
}

/// The environment permissions the artifact requests. Declaring a capability
/// the runtime does not support is a hard admission error (fail-closed).
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CapabilityPolicy {
    /// The versioned intermediary LLM capability (omitted by legacy artifacts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm: Option<String>,
    /// May read the artifact's own payloads.
    pub self_read: bool,
    /// Network access.
    pub network: bool,
    /// Host filesystem access.
    pub filesystem: bool,
    /// Wall-clock access.
    pub clock: bool,
    /// Randomness access.
    pub random: bool,
}

/// Publisher provenance for the three roles.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// The input-data publisher.
    pub data: Publisher,
    /// The algorithm (interface and components) publisher.
    pub algorithm: Publisher,
    /// The artifact publisher who assembled and signed it.
    pub artifact: Publisher,
}

/// One named publisher, optionally carrying a signature reference.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Publisher {
    /// The publisher identity (free-form name).
    pub publisher: String,
    /// The optional signature reference.
    pub signature: Option<String>,
}

impl Publisher {
    /// An anonymous publisher without a signature.
    pub fn empty() -> Self {
        Self {
            publisher: String::new(),
            signature: None,
        }
    }
}

/// A reference to one payload tree.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PayloadRef {
    /// The payload name (unique within the artifact). The first is the default payload.
    pub name: String,
    /// The root node of the Merkle-hashed directory tree.
    pub root: PayloadNode,
    /// The media type of the payload bytes.
    pub media_type: String,
    /// The total real cleartext byte length (sealed artifacts; unused otherwise).
    pub length: u64,
    /// SHA-256 of the tree root (hex).
    pub hash: String,
    /// Compression (reserved in format v1).
    pub compressed: Option<Compression>,
    /// The padded length of the sealed payload blob (sealed artifacts only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padded_length: Option<u64>,
    /// The global ciphertext chunk hashes (sealed artifacts only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunks: Vec<String>,
}

/// One node of a payload tree: a file (with chunk hashes) or a directory
/// (whose hash covers its children).
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PayloadNode {
    /// File or directory.
    pub kind: EntryKind,
    /// The base name within the parent (empty for the root).
    pub name: String,
    /// The content hash (hex).
    pub hash: String,
    /// The byte length (files only).
    pub length: u64,
    /// The chunked-read layout hint (files only).
    pub layout: Option<PayloadLayout>,
    /// Per-chunk SHA-256 (hex), used by multi-chunk files.
    pub chunks: Vec<String>,
    /// The child nodes (directories only).
    pub entries: Vec<PayloadNode>,
}

/// Content hashes of the other sections. The manifest and root hashes are
/// not here (they are derived values).
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SectionHashes {
    /// SHA-256 of the interface section (hex).
    pub interface: String,
    /// SHA-256 of each component (hex), in manifest order.
    pub wasm: Vec<String>,
    /// SHA-256 of each payload tree root (hex).
    pub payload: Vec<String>,
}

/// The signature algorithm.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum SignatureAlgorithm {
    /// ed25519 (RFC 8032, deterministic).
    #[serde(rename = "ed25519")]
    Ed25519,
}

/// One signature over the artifact root hash.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SignatureEntry {
    /// The signature algorithm.
    pub alg: SignatureAlgorithm,
    /// The signer fingerprint: SHA-256 of `pubkey` (hex).
    pub key_id: String,
    /// The signer public key.
    pub pubkey: serde_bytes::ByteBuf,
    /// The signature over the artifact root hash.
    pub sig: serde_bytes::ByteBuf,
}
