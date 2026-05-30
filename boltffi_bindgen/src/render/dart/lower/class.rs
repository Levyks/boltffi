use boltffi_ffi_rules::naming;

use crate::{
    ir::{CallId, ClassDef},
    render::dart::{DartClass, NamingConvention},
};

impl<'a> super::DartLowerer<'a> {
    fn lower_one_class(&self, class: &ClassDef) -> DartClass {
        let constructors = class
            .constructors
            .iter()
            .enumerate()
            .map(|(i, ctor)| {
                self.lower_constructor(
                    ctor,
                    CallId::Constructor {
                        class_id: class.id.clone(),
                        index: i,
                    },
                )
            })
            .collect();

        let methods = class
            .methods
            .iter()
            .map(|meth| {
                self.lower_method(
                    meth,
                    CallId::Method {
                        class_id: class.id.clone(),
                        method_id: meth.id.clone(),
                    },
                )
            })
            .collect();

        DartClass {
            name: NamingConvention::class_name(class.id.as_str()),
            create_symbol: naming::class_ffi_new(class.id.as_str()).to_string(),
            free_symbol: naming::class_ffi_free(class.id.as_str()).to_string(),
            constructors,
            methods,
        }
    }

    pub(super) fn lower_classes(&self) -> Vec<DartClass> {
        self.ffi
            .catalog
            .all_classes()
            .map(|c| self.lower_one_class(c))
            .collect()
    }
}
