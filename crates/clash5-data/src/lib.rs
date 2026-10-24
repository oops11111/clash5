mod chain;
mod identity;
mod plugin_patch;
mod profile;
mod store;

pub use chain::{
    ChainError, CompiledChain, ExitPolicy, Hop, MAX_HOPS, MAX_POOL_MEMBERS, NodePool, NodeRef, ProxyChain, ScopedProxy,
    compile_chains,
};
pub use identity::{DataError, IdentityKey, Node, StableId};
pub use plugin_patch::{
    ConfigProvenance, EffectiveConfig, PluginConflict, PluginPatch, merge_plugin_patches, validate_plugin_patch,
    validate_writable_paths,
};
pub use profile::{parse_inline_nodes, parse_proxy_node, replace_subscription_lines};
pub use store::{
    Binding, BindingStore, IdentitySourceSnapshot, NodeQuality, NodeRecord, OrphanCleanupPreview,
    PendingIdentityRebind, PluginAuditEvent, PluginBinding, PluginBindingTarget, ProviderDiff, QualityStatus, Source,
};
