//! Lowers a scanned Rust source contract into a binding contract for a
//! target [`Surface`].
//!
//! The pass runs once. The returned [`Bindings<S>`] contains the
//! decisions consumers render: direct records carry layout, encoded
//! records carry codec plans, and enums have already been split into
//! c-style or data-bearing forms. Source shapes that do not have a
//! binding-IR representation yet return [`LowerError`] instead of being
//! guessed.
//!
//! # Pipeline
//!
//! 1. Build [`DeclarationIds`] from the source. Duplicate ids in the
//!    same family fail here, before any walk.
//! 2. Build an [`Index`] of the source for cross-decl lookups during
//!    type and codec lowering.
//! 3. Lower every record into [`RecordDecl<S>`], every enum into
//!    [`EnumDecl<S>`], every class into [`ClassDecl<S>`], every trait
//!    into [`CallbackDecl<S>`] (the trait's per-surface dispatch
//!    protocol), every free function into [`FunctionDecl<S>`], every
//!    stream into [`StreamDecl<S>`], every custom type into
//!    [`CustomTypeDecl`], and every constant into [`ConstantDecl<S>`].
//! 4. Hand the collected decls to [`Bindings::from_decls`], which
//!    derives the native symbol table from the symbols the decls
//!    reference and validates the result.
//!
//! Each step in the pipeline returns either final IR or the
//! infrastructure the next step uses; nothing returns a private
//! domain-shaped middle value.
//!
//! The surface is selected at the call site:
//!
//! ```ignore
//! let native = boltffi_binding::lower::<boltffi_binding::Native>(&source)?;
//! let wasm   = boltffi_binding::lower::<boltffi_binding::Wasm32>(&source)?;
//! ```

#![allow(dead_code)]

mod async_protocol;
mod callable;
mod callbacks;
mod classes;
mod codecs;
mod constants;
mod customs;
mod enums;
mod error;
mod functions;
mod ids;
mod index;
mod layout;
mod metadata;
mod methods;
mod names;
mod primitive;
mod records;
mod streams;
mod surface;
mod symbol;
mod types;
mod wasm_closure;

use boltffi_ast::SourceContract;

use crate::{BindingError, Bindings, CanonicalName, Decl, PackageInfo};

pub use self::error::{DeclarationFamily, LowerError, LowerErrorKind, UnsupportedType};
pub use self::surface::SurfaceLower;

use self::{ids::DeclarationIds, index::Index, symbol::SymbolAllocator};

/// Lowers a source contract into a binding contract for surface `S`.
///
/// See the module-level docs for the steps each call runs through.
pub fn lower<S: SurfaceLower>(source: &SourceContract) -> Result<Bindings<S>, LowerError> {
    let ids = DeclarationIds::from_source(source)?;
    let index = Index::new(source);
    let mut allocator = SymbolAllocator::new();

    let records = records::lower::<S>(&index, &ids, &mut allocator)?;
    let enums = enums::lower::<S>(&index, &ids, &mut allocator)?;
    let classes = classes::lower::<S>(&index, &ids, &mut allocator)?;
    let callbacks = callbacks::lower::<S>(&index, &ids, &mut allocator)?;
    let functions = functions::lower::<S>(&index, &ids, &mut allocator)?;
    let streams = streams::lower::<S>(&index, &ids, &mut allocator)?;
    let constants = constants::lower::<S>(&index, &ids, &mut allocator)?;
    let customs = customs::lower(&index, &ids)?;

    let decls = records
        .into_iter()
        .map(|record| Decl::Record(Box::new(record)))
        .chain(
            enums
                .into_iter()
                .map(|enumeration| Decl::Enum(Box::new(enumeration))),
        )
        .chain(
            classes
                .into_iter()
                .map(|class| Decl::Class(Box::new(class))),
        )
        .chain(
            callbacks
                .into_iter()
                .map(|callback| Decl::Callback(Box::new(callback))),
        )
        .chain(
            functions
                .into_iter()
                .map(|function| Decl::Function(Box::new(function))),
        )
        .chain(
            streams
                .into_iter()
                .map(|stream| Decl::Stream(Box::new(stream))),
        )
        .chain(
            customs
                .into_iter()
                .map(|custom| Decl::CustomType(Box::new(custom))),
        )
        .chain(
            constants
                .into_iter()
                .map(|constant| Decl::Constant(Box::new(constant))),
        )
        .collect::<Vec<_>>();

    let package = PackageInfo::new(
        CanonicalName::single(source.package.name.as_str()),
        source.package.version.clone(),
    );

    Ok(Bindings::from_decls(package, decls)?)
}

impl From<BindingError> for LowerError {
    fn from(error: BindingError) -> Self {
        Self::new(LowerErrorKind::InvalidBindings(error))
    }
}
