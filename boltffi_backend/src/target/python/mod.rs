//! Python target rendered through a CPython C extension.

mod cpython;
mod name_style;

pub use cpython::PythonCExtHost;
pub use name_style::PackageModule;
