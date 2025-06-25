// This file manages the Tokio asynchronous runtime for the Rust-Python integration.
// It provides a mechanism to create and access a single, global Tokio runtime instance,
// which is essential for executing asynchronous Rust code (like network operations)
// when called from synchronous Python contexts or when managing shared async resources.

// Import necessary modules from the standard library.
use std::sync::Arc; // `Arc` (Atomically Reference Counted) is used for shared ownership
                    // of the Tokio runtime across multiple threads, ensuring it's safely
                    // accessible from various Python-bound functions.

// Import PyO3 specific modules.
use pyo3::exceptions::PyValueError; // `PyValueError` is used to create Python `ValueError` exceptions
                                   // when an error occurs during runtime creation or access.
use pyo3::prelude::*; // `pyo3::prelude::*` brings in common PyO3 traits and macros.
use pyo3::sync::GILOnceCell; // `GILOnceCell` is a PyO3-specific utility that allows for
                             // one-time initialization of a value that can be safely accessed
                             // across Python's Global Interpreter Lock (GIL) boundaries.
                             // It's crucial for ensuring the Tokio runtime is initialized only once.
use tokio::runtime::Runtime; // `Runtime` is the core Tokio runtime, responsible for
                             // scheduling and executing asynchronous tasks.

// Declare a static `GILOnceCell` to hold the `Arc` to the Tokio `Runtime`.
// `static` ensures it's a single, globally accessible instance.
// `GILOnceCell` ensures that the runtime is initialized exactly once, even if
// `get_runtime` is called concurrently from multiple Python threads.
static RUNTIME: GILOnceCell<Arc<Runtime>> = GILOnceCell::new();

/// `get_runtime` provides a way to retrieve or initialize the global Tokio runtime.
///
/// This function is designed to be called from Python-bound functions that need
/// to execute asynchronous Rust code. It ensures that a single Tokio runtime
/// is created and reused across the entire application's lifetime, which is
/// generally recommended for performance and resource management.
///
/// # Arguments
/// * `py`: The Python GIL token. This is required by `GILOnceCell::get_or_try_init`
///         to ensure GIL safety during initialization.
///
/// # Returns
/// A `PyResult<Arc<Runtime>>` which on success contains an `Arc` to the Tokio `Runtime`,
/// allowing the caller to spawn tasks on it. On failure, it returns a `PyErr`
/// (specifically a `PyValueError`) if the runtime cannot be created.
///
/// # Why this design?
/// - **Single Runtime Instance:** Creating multiple Tokio runtimes can be inefficient
///   and lead to unexpected behavior or resource exhaustion. This design guarantees
///   a single, shared instance.
/// - **Lazy Initialization:** The runtime is only created when it's first needed
///   (`get_or_try_init`), avoiding unnecessary resource allocation if async features
///   are not used.
/// - **GIL Safety:** `GILOnceCell` handles the complexities of Python's GIL, ensuring
///   that the initialization is thread-safe and doesn't cause deadlocks or race conditions
///   when called from multiple Python threads.
/// - **Error Handling:** It gracefully handles potential errors during runtime creation
///   (e.g., if Tokio fails to start) by converting them into Python `ValueError` exceptions.
pub(crate) fn get_runtime(py: Python<'_>) -> PyResult<Arc<Runtime>> {
    // Attempt to get the already initialized runtime, or initialize it if it's the first call.
    let runtime = RUNTIME.get_or_try_init(py, || {
        // This closure is executed only if the runtime has not been initialized yet.
        // It tries to create a new Tokio `Runtime`.
        Ok::<_, PyErr>(Arc::new(Runtime::new().map_err(|err| {
            // If `Runtime::new()` fails, map the Rust error into a Python `PyValueError`.
            PyValueError::new_err(format!("Could not create tokio runtime. {}", err))
        })?))
    })?; // The `?` operator propagates any `PyErr` that might occur during `get_or_try_init` or the closure.

    // Return a clone of the `Arc` to the runtime.
    // Cloning an `Arc` increments its reference count, allowing multiple parts of the code
    // to hold references to the same runtime instance without transferring ownership.
    Ok(runtime.clone())
}