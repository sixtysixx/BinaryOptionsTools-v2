// This is the main library file for the `BinaryOptionsToolsV2` Rust crate,
// which exposes its functionalities to Python using PyO3.
// It acts as the entry point for the Python module, defining what classes,
// functions, and submodules are made available to Python scripts.

// Disables the `non_snake_case` lint for the entire crate. This is often done
// in PyO3 projects to allow Rust code to conform to Python's `CamelCase`
// naming conventions for classes and functions that are exposed to Python,
// while still using `snake_case` for internal Rust-only items.
#![allow(non_snake_case)]

// Declare internal modules that make up the library's structure.
// These modules contain the core logic, data structures, and utilities.
mod config; // Handles configuration settings for the trading client.
mod error; // Defines custom error types for the library, convertible to Python exceptions.
mod logs; // Implements logging functionality, potentially with structured logging and streaming.
mod pocketoption; // Contains logic specific to interacting with the Pocket Option trading platform.
mod runtime; // Manages asynchronous runtime for network operations (e.g., Tokio).
mod stream; // Deals with data streaming, possibly WebSocket message processing.
mod validator; // Provides data validation utilities.

// Bring necessary types into scope from the internal modules for use in `lib.rs`.
// These are the types that will be exposed to the Python module.
use config::PyConfig; // The Python-friendly configuration struct.
use logs::{start_tracing, LogBuilder, Logger, StreamLogsIterator, StreamLogsLayer}; // Logging components.
use pocketoption::{RawPocketOption, RawStreamIterator, StreamIterator}; // Pocket Option specific client and stream iterators.
use pyo3::prelude::*; // Essential PyO3 macros and traits for Python integration.
use validator::RawValidator; // The raw validator class.

/// Defines the Python module named "BinaryOptionsToolsV2".
///
/// `#[pymodule]` is a PyO3 macro that marks a function as the entry point
/// for a Python module. When Python imports `BinaryOptionsToolsV2`, this
/// function is executed to define the module's contents.
///
/// The `m` argument is a `Bound<'_, PyModule>` which represents the Python
/// module object itself, allowing us to add classes and functions to it.
///
/// Returns `PyResult<()>` which indicates success or a Python exception.
#[pymodule]
#[pyo3(name = "BinaryOptionsToolsV2")] // Specifies the name of the Python module.
fn BinaryOptionsTools(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Add Python classes to the module.
    // Each `m.add_class::<T>()?` makes the Rust struct `T` available as a Python class.
    // The `?` operator is used for error propagation, converting Rust `PyErr` into
    // a `PyResult` error.

    // Exposes `StreamLogsIterator` as a Python class, likely for iterating over streamed logs.
    m.add_class::<StreamLogsIterator>()?;
    // Exposes `StreamLogsLayer` as a Python class, probably a component for structured logging.
    m.add_class::<StreamLogsLayer>()?;
    // Exposes `RawPocketOption` as a Python class, serving as the main interface
    // for interacting with the Pocket Option platform from Python.
    m.add_class::<RawPocketOption>()?;
    // Exposes `Logger` as a Python class, providing a way to configure and use the logging system.
    m.add_class::<Logger>()?;
    // Exposes `LogBuilder` as a Python class, likely a builder pattern for creating `Logger` instances.
    m.add_class::<LogBuilder>()?;
    // Exposes `StreamIterator` as a Python class, for general-purpose stream iteration.
    m.add_class::<StreamIterator>()?;
    // Exposes `RawStreamIterator` as a Python class, possibly a lower-level stream iterator.
    m.add_class::<RawStreamIterator>()?;
    // Exposes `RawValidator` as a Python class, providing validation utilities.
    m.add_class::<RawValidator>()?;
    // Exposes `PyConfig` as a Python class, allowing Python users to configure the client.
    m.add_class::<PyConfig>()?;

    // Add Python functions to the module.
    // `wrap_pyfunction!` macro is used to convert a Rust function into a Python callable.
    // `start_tracing` is likely a function to initialize the tracing/logging system.
    m.add_function(wrap_pyfunction!(start_tracing, m)?)?;

    // Return `Ok(())` to indicate that the module was successfully initialized.
    Ok(())
}