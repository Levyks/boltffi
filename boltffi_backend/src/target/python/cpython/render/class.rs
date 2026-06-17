use askama::Template as AskamaTemplate;
use boltffi_binding::{ClassDecl, Native};

use crate::{
    bridge::python_cext::{ExtensionMethod, MethodFlags, PythonCExtBridgeContract},
    core::{Emitted, Error, RenderContext, Result},
    target::python::{
        cpython::render::{argument, function, primitive, result, scalar_handle},
        name_style::Name,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/class_release.c", escape = "none")]
struct ReleaseTemplate {
    python_name: String,
    wrapper: String,
    storage: String,
    handle_type: String,
    parser: String,
}

pub struct Wrapper {
    release: Release,
    callables: Vec<function::Wrapper>,
}

impl Wrapper {
    pub fn from_declaration(
        declaration: &ClassDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = Symbols::new(declaration);
        let initializers = declaration.initializers().iter().map(|initializer| {
            function::Wrapper::from_export(
                symbols.initializer(initializer.name()),
                initializer.symbol(),
                initializer.callable(),
                Vec::new(),
                bridge,
                context,
            )
        });
        let methods = declaration.methods().iter().map(|method| {
            let receiver = method
                .callable()
                .receiver()
                .map(|_| argument::Conversion::class_receiver(declaration.handle()))
                .transpose()?
                .into_iter()
                .collect();
            function::Wrapper::from_export(
                symbols.method(method.name()),
                method.target(),
                method.callable(),
                receiver,
                bridge,
                context,
            )
        });
        Ok(Self {
            release: Release::new(declaration, bridge)?,
            callables: initializers.chain(methods).collect::<Result<Vec<_>>>()?,
        })
    }

    pub fn render(self) -> Result<Emitted> {
        let release = self.release.render()?;
        let callables = self
            .callables
            .into_iter()
            .map(function::Wrapper::render)
            .collect::<Result<Vec<_>>>()?;
        let source = std::iter::once(release)
            .chain(
                callables
                    .into_iter()
                    .map(|emitted| emitted.primary_chunk().as_str().to_owned()),
            )
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Emitted::primary(source))
    }

    pub fn methods(&self) -> impl Iterator<Item = &ExtensionMethod> {
        std::iter::once(self.release.method())
            .chain(self.callables.iter().map(function::Wrapper::method))
    }

    pub fn primitives(&self) -> Vec<primitive::Runtime> {
        self.callables
            .iter()
            .flat_map(function::Wrapper::primitives)
            .chain(std::iter::once(self.release.primitive()))
            .collect()
    }

    pub fn owned_buffers(&self) -> impl Iterator<Item = result::OwnedBuffer> + '_ {
        self.callables
            .iter()
            .filter_map(function::Wrapper::owned_buffer)
    }

    pub fn has_string_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Wrapper::has_string_argument)
    }

    pub fn has_bytes_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Wrapper::has_bytes_argument)
    }
}

pub struct Symbols {
    class_name: String,
    stem: String,
}

impl Symbols {
    pub fn new(declaration: &ClassDecl<Native>) -> Self {
        Self {
            class_name: Name::new(declaration.name()).class(),
            stem: Name::new(declaration.name()).function(),
        }
    }

    pub fn class_name(&self) -> &str {
        &self.class_name
    }

    pub fn initializer(&self, name: &boltffi_binding::CanonicalName) -> String {
        self.callable(name)
    }

    pub fn method(&self, name: &boltffi_binding::CanonicalName) -> String {
        self.callable(name)
    }

    pub fn release(&self) -> String {
        format!("_boltffi_{}_release", self.stem)
    }

    fn callable(&self, name: &boltffi_binding::CanonicalName) -> String {
        format!("_boltffi_{}_{}", self.stem, Name::new(name).function())
    }
}

struct Release {
    python_name: String,
    wrapper: String,
    storage: String,
    handle: scalar_handle::Carrier,
    method: ExtensionMethod,
}

impl Release {
    fn new(declaration: &ClassDecl<Native>, bridge: &PythonCExtBridgeContract) -> Result<Self> {
        let symbols = Symbols::new(declaration);
        let python_name = symbols.release();
        let wrapper = format!(
            "boltffi_python_callable_wrapper_{}",
            declaration.release().name().as_str()
        );
        let loaded =
            bridge
                .loaded_function(declaration.release())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "class release without C bridge symbol",
                })?;
        let method =
            ExtensionMethod::new(python_name.clone(), wrapper.clone(), MethodFlags::FastCall)?;
        Ok(Self {
            python_name,
            wrapper,
            storage: loaded.storage_name().to_owned(),
            handle: scalar_handle::Carrier::new(declaration.handle())?,
            method,
        })
    }

    fn render(self) -> Result<String> {
        Ok(ReleaseTemplate {
            python_name: self.python_name,
            wrapper: self.wrapper,
            storage: self.storage,
            handle_type: self.handle.c_type()?.to_owned(),
            parser: self.handle.parser()?.to_owned(),
        }
        .render()?)
    }

    fn method(&self) -> &ExtensionMethod {
        &self.method
    }

    fn primitive(&self) -> primitive::Runtime {
        self.handle.primitive()
    }
}
