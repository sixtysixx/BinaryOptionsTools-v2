// stream.rs
// This file provides utility functions and type aliases for working with asynchronous streams
// in the context of PyO3, specifically focusing on how to consume stream items and
// propagate stream termination or errors back to Python's iteration protocols.

// Standard library imports.
use std::sync::Arc; // `Arc` for shared ownership of stream instances across threads/tasks.

// Imports from `futures_util` for asynchronous stream manipulation.
use futures_util::{
    stream::{
        BoxStream, // A type alias for a trait object representing a stream.
        Fuse,      // Used to make a stream "fuse" (stop yielding items) after it returns `None`.
    },
    StreamExt, // Provides useful methods for streams, like `next()`.
};
// PyO3 imports for handling Python exceptions.
use pyo3::{
    exceptions::{PyStopAsyncIteration, PyStopIteration}, // Specific Python exceptions for iteration termination.
    PyResult, // A Result type for PyO3 operations, converting Rust errors to Python exceptions.
};
// Tokio specific imports for asynchronous programming.
use tokio::sync::Mutex; // An asynchronous mutex for protecting shared stream state in async contexts.

/// `PyStream` is a type alias for a common pattern of asynchronous streams used in this project.
///
/// It represents a stream that:
/// - Is `Fuse`d: Once it returns `None` (indicating exhaustion), it will always return `None`.
///   This is good practice for iterators to ensure predictable behavior.
/// - Is `BoxStream`: It's a trait object, allowing for dynamic dispatch of stream implementations.
///   The `'static` lifetime means the stream doesn't hold references to data with shorter lifetimes,
///   making it suitable for long-lived background tasks.
/// - Yields `Result<T, E>`: Each item from the stream is a `Result`, allowing for
///   error propagation within the stream itself. `T` is the success type, and `E` is the error type.
pub type PyStream<T, E> = Fuse<BoxStream<'static, Result<T, E>>>;

/// `next_stream` is an asynchronous helper function to get the next item from a `PyStream`.
/// It handles both successful item retrieval, stream exhaustion, and error propagation,
/// converting them into appropriate Python exceptions (`PyStopIteration` or `PyStopAsyncIteration`).
///
/// # Arguments
/// * `stream`: An `Arc<Mutex<PyStream<T, E>>>` representing the shared, mutable stream.
///             The `Arc` allows shared ownership, and `Mutex` protects concurrent access
///             to the stream from different async tasks.
/// * `sync`: A boolean flag indicating whether the caller is expecting a synchronous
///           (`true`) or asynchronous (`false`) iteration termination. This affects
///           which `StopIteration` exception type is raised.
///
/// # Type Parameters
/// * `T`: The type of the successful item yielded by the stream.
/// * `E`: The error type that the stream can yield. This type must implement `std::error::Error`
///        so its error message can be converted to a string for Python exceptions.
///
/// # Returns
/// A `PyResult<T>`:
/// - `Ok(T)`: If the stream yields a successful item.
/// - `Err(PyStopIteration)`: If the stream is exhausted and `sync` is `true`.
/// - `Err(PyStopAsyncIteration)`: If the stream is exhausted and `sync` is `false`.
/// - `Err(PyStopIteration)` or `Err(PyStopAsyncIteration)`: If the stream yields an error,
///   the error message from `E` is used for the Python exception.
///
/// # Why this design?
/// This function centralizes the logic for consuming stream items and mapping Rust's
/// `Result` and stream termination (`None`) to Python's iteration protocol exceptions.
/// This reduces boilerplate code in `__next__` and `__anext__` implementations for PyO3-exposed
/// iterators, ensuring consistent error handling and stream behavior.
pub async fn next_stream<T, E>(stream: Arc<Mutex<PyStream<T, E>>>, sync: bool) -> PyResult<T>
where
    E: std::error::Error, // Constraint: The error type `E` must implement `std::error::Error`
                          // so we can call `to_string()` on it for the Python exception message.
{
    // Acquire a lock on the mutex protecting the stream. `await` here means
    // this task will yield until the lock is available.
    let mut stream = stream.lock().await;

    // Attempt to get the next item from the stream.
    match stream.next().await {
        // If an item is received (`Some(item)`).
        Some(item) => match item {
            // If the item is a success (`Ok(itm)`).
            Ok(itm) => Ok(itm), // Return the successful item.
            // If the item is an error (`Err(e)`).
            Err(e) => {
                // Print the error to standard output for debugging purposes.
                println!("Error: {:?}", e);
                // Based on the `sync` flag, return the appropriate Python `StopIteration` exception
                // with the error message from the Rust error.
                match sync {
                    true => Err(PyStopIteration::new_err(e.to_string())),
                    false => Err(PyStopAsyncIteration::new_err(e.to_string())),
                }
            }
        },
        // If the stream is exhausted (`None`).
        None => {
            // Based on the `sync` flag, return the appropriate Python `StopIteration` exception
            // indicating that the stream has no more items.
            match sync {
                true => Err(PyStopIteration::new_err("Stream exhausted")),
                false => Err(PyStopAsyncIteration::new_err("Stream exhausted")),
            }
        }
    }
}