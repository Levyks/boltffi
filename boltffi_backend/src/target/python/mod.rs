//! Python target rendered through a CPython C extension.

mod codec;
mod cpython;
mod name_style;
mod render;

pub use cpython::PythonCExtHost;
pub use name_style::PackageModule;
