//! The unstructured level: pipelines written in Python by the format's author.
//!
//! "Unstructured" means a segment's algorithm is unique to that format and does not lend itself to
//! parameterisation: the only way to express it is to write the code, and that code lives in the
//! formats repository rather than in the library. It is one of the crate's two points of contact
//! with Python, the other being loading the PDF.
//!
//! - [`loader`] finds and imports the format's Python module and reads its pipelines and page-class function;
//! - [`py_pipe`] wraps the callables that come out in the pipe traits, so that the engine cannot tell an author's pipe from a native one.

pub mod loader;
pub mod py_pipe;
