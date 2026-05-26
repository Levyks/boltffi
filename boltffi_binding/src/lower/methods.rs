//! Method and initializer lowering for declaration-owned callables.
//!
//! Records, enums, and classes can promote static `Self`-returning methods to
//! [`InitializerDecl<S>`]. Records, enums, and classes keep every other
//! method as a [`MethodDecl<S, NativeSymbol>`]. The callable body is
//! lowered by [`super::callable`]; this module owns the initializer
//! discriminator, target symbol allocation, and owner-specific constructed
//! type recorded on an initializer.

use boltffi_ast::{
    ClassDef, EnumDef, MethodDef, Receiver, RecordDef, ReturnDef, TraitDef, TypeExpr,
};

use crate::{
    CanonicalName, InitializerDecl, InitializerId, MethodDecl, MethodId, NativeSymbol,
    ReturnTypeRef, TypeRef,
};

use super::{
    LowerError, callable,
    error::UnsupportedType,
    ids::DeclarationIds,
    index::Index,
    metadata,
    surface::SurfaceLower,
    symbol::{
        CallbackSlot, SymbolAllocator, SymbolOwner, initializer_symbol_name, member_symbol_name,
    },
};

/// Lowers every initializer-shaped method on `record`.
///
/// Initializer ids are assigned after non-initializer methods are removed,
/// so the initializer table is dense in the exact order renderers observe.
pub(super) fn lower_record_initializers<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    record: &RecordDef,
) -> Result<Vec<InitializerDecl<S>>, LowerError> {
    let owner = callable::CallableOwner::Record(record);
    let record_id = ids.record(&record.id)?;
    lower_initializers(
        idx,
        ids,
        allocator,
        owner,
        &record.methods,
        TypeRef::Record(record_id),
    )
}

/// Lowers every non-initializer method on `record`.
///
/// Method ids are assigned after initializer-shaped methods are removed,
/// so the method table is dense in the exact order renderers observe.
pub(super) fn lower_record_methods<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    record: &RecordDef,
) -> Result<Vec<MethodDecl<S, NativeSymbol>>, LowerError> {
    let owner = callable::CallableOwner::Record(record);
    record
        .methods
        .iter()
        .filter(|method| !is_initializer(owner, method))
        .enumerate()
        .map(|(index, method)| {
            lower_method::<S>(
                idx,
                ids,
                allocator,
                owner,
                method,
                MethodId::from_raw(index as u32),
            )
        })
        .collect()
}

/// Lowers every initializer-shaped method on `enumeration`.
pub(super) fn lower_enum_initializers<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    enumeration: &EnumDef,
) -> Result<Vec<InitializerDecl<S>>, LowerError> {
    let owner = callable::CallableOwner::Enum(enumeration);
    let enum_id = ids.enumeration(&enumeration.id)?;
    lower_initializers(
        idx,
        ids,
        allocator,
        owner,
        &enumeration.methods,
        TypeRef::Enum(enum_id),
    )
}

/// Lowers every non-initializer method on `enumeration`.
pub(super) fn lower_enum_methods<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    enumeration: &EnumDef,
) -> Result<Vec<MethodDecl<S, NativeSymbol>>, LowerError> {
    let owner = callable::CallableOwner::Enum(enumeration);
    enumeration
        .methods
        .iter()
        .filter(|method| !is_initializer(owner, method))
        .enumerate()
        .map(|(index, method)| {
            lower_method::<S>(
                idx,
                ids,
                allocator,
                owner,
                method,
                MethodId::from_raw(index as u32),
            )
        })
        .collect()
}

/// Lowers every initializer-shaped method on `class`.
///
/// Class initializers construct the class handle target rather than a
/// value-shaped record. The callable still carries the native crossing
/// selected for the `Self` return.
pub(super) fn lower_class_initializers<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    class: &ClassDef,
) -> Result<Vec<InitializerDecl<S>>, LowerError> {
    let owner = callable::CallableOwner::Class(class);
    let class_id = ids.class(&class.id)?;
    lower_initializers(
        idx,
        ids,
        allocator,
        owner,
        &class.methods,
        TypeRef::Class(class_id),
    )
}

/// Lowers every non-initializer method on `class`.
///
/// Owned class receivers are rejected until the handle ownership-transfer
/// protocol is represented in the binding IR.
pub(super) fn lower_class_methods<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    class: &ClassDef,
) -> Result<Vec<MethodDecl<S, NativeSymbol>>, LowerError> {
    let owner = callable::CallableOwner::Class(class);
    class
        .methods
        .iter()
        .filter(|method| !is_initializer(owner, method))
        .enumerate()
        .map(|(index, method)| {
            reject_owned_class_receiver(method)?;
            lower_method::<S>(
                idx,
                ids,
                allocator,
                owner,
                method,
                MethodId::from_raw(index as u32),
            )
        })
        .collect()
}

fn is_initializer(owner: callable::CallableOwner<'_>, method: &MethodDef) -> bool {
    matches!(method.receiver, Receiver::None)
        && match &method.returns {
            ReturnDef::Value(type_expr) => returns_owner(owner, type_expr),
            ReturnDef::Void => false,
        }
}

fn returns_owner(owner: callable::CallableOwner<'_>, type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Result { ok, .. } => owner.owns_type_expr(ok),
        other => owner.owns_type_expr(other),
    }
}

fn lower_initializers<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: callable::CallableOwner<'_>,
    methods: &[MethodDef],
    returns: TypeRef,
) -> Result<Vec<InitializerDecl<S>>, LowerError> {
    methods
        .iter()
        .filter(|method| is_initializer(owner, method))
        .enumerate()
        .map(|(index, method)| {
            lower_initializer(
                idx,
                ids,
                allocator,
                owner,
                method,
                InitializerId::from_raw(index as u32),
                returns.clone(),
            )
        })
        .collect()
}

fn lower_initializer<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: callable::CallableOwner<'_>,
    method: &MethodDef,
    id: InitializerId,
    returns: TypeRef,
) -> Result<InitializerDecl<S>, LowerError> {
    let callable_decl = callable::lower_method::<S>(idx, ids, owner, method)?;
    let symbol = mint_initializer_symbol(allocator, owner, method)?;
    Ok(InitializerDecl::new(
        id,
        CanonicalName::from(&method.name),
        metadata::decl_meta(method.doc.as_ref(), method.deprecated.as_ref()),
        symbol,
        callable_decl,
        ReturnTypeRef::Value(returns),
    ))
}

fn lower_method<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: callable::CallableOwner<'_>,
    method: &MethodDef,
    id: MethodId,
) -> Result<MethodDecl<S, NativeSymbol>, LowerError> {
    let callable_decl = callable::lower_method::<S>(idx, ids, owner, method)?;
    let symbol = mint_method_symbol(allocator, owner, method)?;
    Ok(MethodDecl::new(
        id,
        CanonicalName::from(&method.name),
        metadata::decl_meta(method.doc.as_ref(), method.deprecated.as_ref()),
        symbol,
        callable_decl,
    ))
}

fn mint_method_symbol(
    allocator: &mut SymbolAllocator,
    owner: callable::CallableOwner<'_>,
    method: &MethodDef,
) -> Result<NativeSymbol, LowerError> {
    let method_name = method.name.parts().last().map_or("", |part| part.as_str());
    let symbol_name = member_symbol_name(symbol_owner(owner), method_name);
    allocator.mint(symbol_name)
}

fn mint_initializer_symbol(
    allocator: &mut SymbolAllocator,
    owner: callable::CallableOwner<'_>,
    method: &MethodDef,
) -> Result<NativeSymbol, LowerError> {
    let method_name = method.name.parts().last().map_or("", |part| part.as_str());
    let symbol_name = initializer_symbol_name(symbol_owner(owner), method_name);
    allocator.mint(symbol_name)
}

/// Lowers every method on `callback` with a per-surface dispatch
/// target.
///
/// Records and enums and classes all use [`NativeSymbol`] as the
/// method target because their methods are Rust-implemented. Callback
/// traits invert ownership: foreign code provides each method, so the
/// target is surface-divergent — a [`crate::VTableSlot`] on native, a
/// [`crate::ImportSymbol`] on wasm32.
///
/// The `target_for` closure receives a [`CallbackSlot`], not a raw
/// string. The slot is constructed once here from the source method
/// ident, so the type system guarantees the value handed to each
/// surface is the canonical snake-cased name. A future caller of the
/// per-surface name constructors cannot bypass normalization because
/// they cannot construct a [`CallbackSlot`] from arbitrary text.
pub(super) fn lower_callback_methods<S: SurfaceLower, T, F>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    source_trait: &TraitDef,
    mut target_for: F,
) -> Result<Vec<MethodDecl<S, T>>, LowerError>
where
    F: FnMut(&CallbackSlot) -> Result<T, LowerError>,
{
    let owner = callable::CallableOwner::Trait(source_trait);
    source_trait
        .methods
        .iter()
        .enumerate()
        .map(|(index, method)| {
            require_callback_receiver(method.receiver)?;
            let callable_decl = callable::lower_method::<S>(idx, ids, owner, method)?;
            let raw_method_name = method.name.parts().last().map_or("", |part| part.as_str());
            let slot = CallbackSlot::from_method_name(raw_method_name);
            let target = target_for(&slot)?;
            Ok(MethodDecl::new(
                MethodId::from_raw(index as u32),
                CanonicalName::from(&method.name),
                metadata::decl_meta(method.doc.as_ref(), method.deprecated.as_ref()),
                target,
                callable_decl,
            ))
        })
        .collect()
}

fn require_callback_receiver(receiver: Receiver) -> Result<(), LowerError> {
    match receiver {
        Receiver::Shared | Receiver::Mutable => Ok(()),
        Receiver::None | Receiver::Owned => Err(LowerError::unsupported_type(
            UnsupportedType::InvalidCallbackReceiver,
        )),
    }
}

fn symbol_owner(owner: callable::CallableOwner<'_>) -> SymbolOwner<'_> {
    match owner {
        callable::CallableOwner::Record(record) => SymbolOwner::record(record.id.as_str()),
        callable::CallableOwner::Enum(enumeration) => {
            SymbolOwner::enumeration(enumeration.id.as_str())
        }
        callable::CallableOwner::Class(class) => SymbolOwner::class(class.id.as_str()),
        callable::CallableOwner::Trait(source_trait) => {
            SymbolOwner::callback(source_trait.id.as_str())
        }
        callable::CallableOwner::Function => unreachable!("free functions do not own methods"),
    }
}

fn reject_owned_class_receiver(method: &MethodDef) -> Result<(), LowerError> {
    if matches!(method.receiver, Receiver::Owned) {
        Err(LowerError::unsupported_type(
            UnsupportedType::OwnedClassReceiver,
        ))
    } else {
        Ok(())
    }
}
