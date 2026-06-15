use std::collections::BTreeMap;
use std::collections::BTreeSet;

use boltffi_binding::{Bindings, Decl, Surface};

use crate::{Error, Result};

/// A binding-contract feature a host renderer may or may not support.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum BindingCapability {
    /// Record declarations.
    Records,
    /// Enum declarations.
    Enums,
    /// Free function declarations.
    Functions,
    /// Class declarations.
    Classes,
    /// Callback trait declarations.
    Callbacks,
    /// Stream declarations.
    Streams,
    /// Constant declarations.
    Constants,
    /// Custom type declarations.
    CustomTypes,
}

/// A bridge feature a host renderer may require from its bridge stack.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum BridgeCapability {
    /// C ABI declarations and symbols.
    CAbi,
    /// JNI method and callback surface.
    Jni,
    /// Wasm exports, imports, and linear-memory protocol.
    Wasm,
    /// CPython extension surface.
    PythonExtension,
    /// N-API surface.
    Napi,
}

/// Support state for one backend capability.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum CapabilityStatus {
    /// The capability is production-supported.
    Stable,
    /// The capability can be rendered behind an explicit experimental policy.
    Experimental {
        /// Reason shown in diagnostics.
        reason: &'static str,
    },
    /// The capability is under active implementation and not release-ready.
    InProgress {
        /// Reason shown in diagnostics.
        reason: &'static str,
    },
    /// The capability cannot be rendered by this backend.
    Unsupported {
        /// Reason shown in diagnostics.
        reason: &'static str,
    },
}

impl CapabilityStatus {
    /// Returns whether this status satisfies a required stable capability.
    pub const fn is_stable(self) -> bool {
        matches!(self, Self::Stable)
    }
}

/// Capability table for one backend subject.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CapabilitySet<C> {
    statuses: BTreeMap<C, CapabilityStatus>,
}

/// Capabilities required by a binding contract or host renderer.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CapabilityRequirements<C> {
    capabilities: BTreeSet<C>,
}

impl<C> Default for CapabilityRequirements<C> {
    fn default() -> Self {
        Self {
            capabilities: BTreeSet::new(),
        }
    }
}

impl<C> CapabilityRequirements<C>
where
    C: Copy + Ord,
{
    /// Creates an empty requirement set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a required capability.
    pub fn require(mut self, capability: C) -> Self {
        self.capabilities.insert(capability);
        self
    }

    /// Iterates over required capabilities.
    pub fn iter(&self) -> impl Iterator<Item = C> + '_ {
        self.capabilities.iter().copied()
    }

    /// Returns whether the set contains no requirements.
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }
}

impl CapabilityRequirements<BindingCapability> {
    /// Builds binding requirements from the declarations in a contract.
    pub fn from_bindings<S: Surface>(target: &'static str, bindings: &Bindings<S>) -> Result<Self> {
        bindings
            .decls()
            .iter()
            .try_fold(Self::new(), |requirements, decl| {
                BindingCapability::from_decl(target, decl)
                    .map(|capability| requirements.require(capability))
            })
    }
}

impl<C> Default for CapabilitySet<C> {
    fn default() -> Self {
        Self {
            statuses: BTreeMap::new(),
        }
    }
}

impl<C> CapabilitySet<C>
where
    C: Copy + Ord,
{
    /// Creates an empty capability table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks a capability as stable.
    pub fn stable(mut self, capability: C) -> Self {
        self.statuses.insert(capability, CapabilityStatus::Stable);
        self
    }

    /// Marks a capability as experimental.
    pub fn experimental(mut self, capability: C, reason: &'static str) -> Self {
        self.statuses
            .insert(capability, CapabilityStatus::Experimental { reason });
        self
    }

    /// Marks a capability as in progress.
    pub fn in_progress(mut self, capability: C, reason: &'static str) -> Self {
        self.statuses
            .insert(capability, CapabilityStatus::InProgress { reason });
        self
    }

    /// Marks a capability as unsupported.
    pub fn unsupported(mut self, capability: C, reason: &'static str) -> Self {
        self.statuses
            .insert(capability, CapabilityStatus::Unsupported { reason });
        self
    }

    /// Returns the recorded support state for a capability.
    pub fn status(&self, capability: C) -> CapabilityStatus {
        self.statuses
            .get(&capability)
            .copied()
            .unwrap_or(CapabilityStatus::Unsupported {
                reason: "capability was not advertised",
            })
    }
}

impl CapabilitySet<BindingCapability> {
    /// Requires every listed binding capability to be stable.
    pub fn require_binding(
        &self,
        target: &'static str,
        required: &CapabilityRequirements<BindingCapability>,
    ) -> Result<()> {
        required.iter().try_for_each(|capability| {
            let status = self.status(capability);
            if status.is_stable() {
                Ok(())
            } else {
                Err(Error::BindingCapability {
                    target,
                    capability,
                    status,
                })
            }
        })
    }
}

impl CapabilitySet<BridgeCapability> {
    /// Requires every listed bridge capability to be stable.
    pub fn require_bridge(
        &self,
        target: &'static str,
        required: &CapabilityRequirements<BridgeCapability>,
    ) -> Result<()> {
        required.iter().try_for_each(|capability| {
            let status = self.status(capability);
            if status.is_stable() {
                Ok(())
            } else {
                Err(Error::BridgeCapability {
                    target,
                    capability,
                    status,
                })
            }
        })
    }
}

impl BindingCapability {
    fn from_decl<S: Surface>(target: &'static str, decl: &Decl<S>) -> Result<Self> {
        match decl {
            Decl::Record(_) => Ok(Self::Records),
            Decl::Enum(_) => Ok(Self::Enums),
            Decl::Function(_) => Ok(Self::Functions),
            Decl::Class(_) => Ok(Self::Classes),
            Decl::Callback(_) => Ok(Self::Callbacks),
            Decl::Stream(_) => Ok(Self::Streams),
            Decl::Constant(_) => Ok(Self::Constants),
            Decl::CustomType(_) => Ok(Self::CustomTypes),
            _ => Err(Error::UnsupportedDeclaration {
                target,
                declaration: decl.id(),
            }),
        }
    }
}
