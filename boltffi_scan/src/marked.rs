use crate::marker::Marker;
use crate::source_tree::SourceTree;
use crate::{ModulePath, ScanError};

pub(super) struct MarkedItems<'source> {
    records: Vec<Marked<'source, syn::ItemStruct>>,
    enums: Vec<Marked<'source, syn::ItemEnum>>,
    functions: Vec<Marked<'source, syn::ItemFn>>,
    traits: Vec<Marked<'source, syn::ItemTrait>>,
    impls: Vec<Marked<'source, syn::ItemImpl>>,
}

impl<'source> MarkedItems<'source> {
    pub(super) fn collect(tree: &'source SourceTree) -> Result<Self, ScanError> {
        tree.modules()
            .iter()
            .flat_map(|module| module.items().iter().map(move |item| (module.path(), item)))
            .try_fold(Self::empty(), |mut marked, (module, item)| {
                marked.push(module, item)?;
                Ok(marked)
            })
    }

    fn empty() -> Self {
        Self {
            records: Vec::new(),
            enums: Vec::new(),
            functions: Vec::new(),
            traits: Vec::new(),
            impls: Vec::new(),
        }
    }

    pub(super) fn records(&self) -> &[Marked<'source, syn::ItemStruct>] {
        &self.records
    }

    pub(super) fn enums(&self) -> &[Marked<'source, syn::ItemEnum>] {
        &self.enums
    }

    pub(super) fn functions(&self) -> &[Marked<'source, syn::ItemFn>] {
        &self.functions
    }

    pub(super) fn traits(&self) -> &[Marked<'source, syn::ItemTrait>] {
        &self.traits
    }

    pub(super) fn impls(&self) -> &[Marked<'source, syn::ItemImpl>] {
        &self.impls
    }

    fn push(
        &mut self,
        module: &'source ModulePath,
        item: &'source syn::Item,
    ) -> Result<(), ScanError> {
        let Some(marker) = Marker::detect(attrs(item))? else {
            return Ok(());
        };
        match (marker, item) {
            (Marker::Data | Marker::Error, syn::Item::Struct(item)) => {
                self.records.push(Marked::new(module, marker, item));
                Ok(())
            }
            (Marker::Data | Marker::Error, syn::Item::Enum(item)) => {
                self.enums.push(Marked::new(module, marker, item));
                Ok(())
            }
            (Marker::DataImpl, syn::Item::Impl(item)) => {
                self.impls.push(Marked::new(module, marker, item));
                Ok(())
            }
            (Marker::Export, syn::Item::Fn(item)) => {
                self.functions.push(Marked::new(module, marker, item));
                Ok(())
            }
            (Marker::Export, syn::Item::Trait(item)) => {
                self.traits.push(Marked::new(module, marker, item));
                Ok(())
            }
            _ => Err(ScanError::InvalidMarkerPlacement {
                marker: marker.as_str().to_owned(),
                item: item_kind(item).to_owned(),
            }),
        }
    }
}

pub(super) struct Marked<'source, T> {
    module: &'source ModulePath,
    marker: Marker,
    item: &'source T,
}

impl<'source, T> Marked<'source, T> {
    fn new(module: &'source ModulePath, marker: Marker, item: &'source T) -> Self {
        Self {
            module,
            marker,
            item,
        }
    }

    pub(super) fn module(&self) -> &'source ModulePath {
        self.module
    }

    pub(super) fn marker(&self) -> Marker {
        self.marker
    }

    pub(super) fn item(&self) -> &'source T {
        self.item
    }
}

fn attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::ExternCrate(item) => &item.attrs,
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::ForeignMod(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        syn::Item::Macro(item) => &item.attrs,
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Static(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Trait(item) => &item.attrs,
        syn::Item::TraitAlias(item) => &item.attrs,
        syn::Item::Type(item) => &item.attrs,
        syn::Item::Union(item) => &item.attrs,
        syn::Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn item_kind(item: &syn::Item) -> &'static str {
    match item {
        syn::Item::Const(_) => "const",
        syn::Item::Enum(_) => "enum",
        syn::Item::ExternCrate(_) => "extern crate",
        syn::Item::Fn(_) => "function",
        syn::Item::ForeignMod(_) => "foreign mod",
        syn::Item::Impl(_) => "impl",
        syn::Item::Macro(_) => "macro",
        syn::Item::Mod(_) => "module",
        syn::Item::Static(_) => "static",
        syn::Item::Struct(_) => "struct",
        syn::Item::Trait(_) => "trait",
        syn::Item::TraitAlias(_) => "trait alias",
        syn::Item::Type(_) => "type alias",
        syn::Item::Union(_) => "union",
        syn::Item::Use(_) => "use",
        _ => "item",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_tree::SourceTree;

    fn tree(source: &str) -> SourceTree {
        let file = syn::parse_str::<syn::File>(source).expect("valid source");
        SourceTree::in_memory("demo", file.items).expect("source tree")
    }

    #[test]
    fn collects_marked_items_by_domain_shape() {
        let tree = tree(
            "#[data] struct Point { x: i32 } \
             #[error] enum ParseError { Eof } \
             #[export] fn origin() {} \
             #[export] trait Listener { fn call(&self); } \
             #[data(impl)] impl Point {}",
        );

        let marked = MarkedItems::collect(&tree).expect("marked items");

        assert_eq!(marked.records().len(), 1);
        assert_eq!(marked.records()[0].marker(), Marker::Data);
        assert_eq!(marked.enums().len(), 1);
        assert_eq!(marked.enums()[0].marker(), Marker::Error);
        assert_eq!(marked.functions().len(), 1);
        assert_eq!(marked.traits().len(), 1);
        assert_eq!(marked.impls().len(), 1);
    }

    #[test]
    fn rejects_marker_on_wrong_item_kind() {
        let tree = tree("#[export] struct Point { x: i32 }");

        let error = match MarkedItems::collect(&tree) {
            Ok(_) => panic!("wrong marker placement must reject"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "export".to_owned(),
                item: "struct".to_owned()
            }
        );
    }
}
