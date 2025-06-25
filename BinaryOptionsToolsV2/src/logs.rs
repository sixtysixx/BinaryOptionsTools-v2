// This file is responsible for setting up and managing the structured logging
// infrastructure for the `BinaryOptionsToolsV2` application. It integrates
// the `tracing` crate for robust and flexible logging, allowing logs to be
// directed to files, the terminal, or even streamed back to Python.
// It provides Python-callable functions and classes to configure and interact
// with this logging system.

// Standard library imports for file operations and synchronization primitives.
use std::{
    fs::OpenOptions, // Used for opening and creating log files with specific options (e.g., append).
    io::Write,         // Provides the `Write` trait for writing to files or other outputs.
    sync::Arc,         // Used for shared ownership of data across threads, particularly for layers and streams.
};

// Imports from the `binary_options_tools` crate.
use binary_options_tools::{
    error::BinaryOptionsResult, // Custom Result type for operations within the binary options tools.
    stream::{
        stream_logs_layer, // A function to create a tracing layer that streams logs.
        RecieverStream,    // A stream type that receives log messages.
    },
};
// `chrono::Duration` is used for time durations, specifically for log stream timeouts.
use chrono::Duration;
// Imports from `futures_util` for asynchronous stream manipulation.
use futures_util::{
    stream::{
        BoxStream, // A type alias for a trait object representing a stream.
        Fuse,      // Used to make a stream "fuse" (stop yielding items) after it returns `None`.
    },
    StreamExt, // Provides useful methods for streams, like `boxed()` and `fuse()`.
};
// PyO3 imports for exposing Rust functionality to Python.
use pyo3::{
    pyclass,      // Macro to mark a struct as a Python class.
    pyfunction,   // Macro to mark a function as a Python function.
    pymethods,    // Macro to define methods for a Python class.
    Bound,        // Represents a reference to a Python object with a lifetime.
    Py,           // A managed pointer to a Python object.
    PyAny,        // A dynamically typed Python object.
    PyResult,     // A Result type for PyO3 operations, converting Rust errors to Python exceptions.
    Python,       // A token representing the Python interpreter's GIL.
};
// `pyo3_async_runtimes` for integrating Rust async functions with Python's asyncio.
use pyo3_async_runtimes::tokio::future_into_py; // Converts a Rust future into a Python awaitable.
// Tokio specific imports for asynchronous programming.
use tokio::sync::Mutex; // An asynchronous mutex for protecting shared data in async contexts.
// Tracing imports for structured logging.
use tracing::{
    debug,           // Macro for debug-level logging.
    instrument,      // Macro to automatically create spans for functions.
    level_filters::LevelFilter, // Used to filter logs based on their severity level.
    warn,            // Macro for warning-level logging.
    Level,           // Enum representing standard logging levels (e.g., DEBUG, INFO, ERROR).
};
// Tracing subscriber imports for configuring the logging backend.
use tracing_subscriber::{
    fmt::{
        self,      // Formatter for log records.
        MakeWriter, // Trait for creating writers for log output.
    },
    layer::SubscriberExt, // Trait to extend `tracing_subscriber::Registry` with layers.
    util::SubscriberInitExt, // Trait to initialize the global default subscriber.
    Layer,                   // Trait for a `tracing` subscriber layer.
    Registry,                // The core `tracing` subscriber that layers attach to.
};

// Internal crate imports.
use crate::{
    error::BinaryErrorPy, // Custom error type for PyO3 compatibility.
    runtime::get_runtime, // Function to get the Tokio runtime instance.
    stream::next_stream,  // Helper function to get the next item from a stream.
};

// Defines a constant `TARGET` string used by `tracing::instrument` macros.
// This helps categorize logs originating from Python-exposed functions.
const TARGET: &str = "Python";

/// Initializes the global tracing subscriber with various layers based on configuration.
/// This function is exposed to Python to allow users to set up their logging.
///
/// # Arguments
/// * `path`: The base directory path where log files (`error.log`, `logs.log`) will be created.
/// * `level`: The default minimum log level (e.g., "DEBUG", "INFO", "WARN", "ERROR") for file and terminal output.
/// * `terminal`: A boolean indicating whether logs should also be output to the terminal (stdout).
/// * `layers`: A list of `StreamLogsLayer` objects, which are pre-configured tracing layers
///             (e.g., for streaming logs back to Python).
///
/// # Why this design?
/// This function provides a flexible way to configure the `tracing` ecosystem.
/// By allowing multiple layers (file, terminal, stream) and dynamic log levels,
/// it caters to different deployment and debugging scenarios. The use of `tracing_subscriber`
/// ensures a powerful and extensible logging setup.
#[pyfunction]
pub fn start_tracing(
    path: String,
    level: String,
    terminal: bool,
    layers: Vec<StreamLogsLayer>,
) -> PyResult<()> {
    // Parse the log level string into a `LevelFilter`. If parsing fails, default to DEBUG.
    let level: LevelFilter = level.parse().unwrap_or(Level::DEBUG.into());

    // Open (or create) the `error.log` file in append mode. This file will specifically
    // capture warning and error level logs.
    let error_logs = OpenOptions::new()
        .append(true)
        .create(true)
        .open(format!("{}/error.log", &path))?;
    // Open (or create) the `logs.log` file in append mode. This file will capture
    // logs at or above the specified `level`.
    let logs = OpenOptions::new()
        .append(true)
        .create(true)
        .open(format!("{}/logs.log", &path))?;

    // Create a default `fmt::Layer` that writes to `NoneWriter`.
    // This layer acts as a "catch-all" or "discard" layer. It's pushed last
    // to ensure that if no other layer processes a log record, it doesn't
    // fall through to a default `tracing-subscriber` behavior that might print to stderr
    // if not explicitly configured, especially when custom layers are in use.
    let default = fmt::Layer::default().with_writer(NoneWriter).boxed();

    // Unwrap the `Arc` from `StreamLogsLayer` to get the actual boxed `Layer` trait objects.
    // This is necessary because `StreamLogsLayer` is a PyO3-exposed wrapper around an `Arc<Box<dyn Layer>>`.
    let mut layers = layers
        .into_iter()
        .flat_map(|l| Arc::try_unwrap(l.layer))
        .collect::<Vec<Box<dyn Layer<Registry> + Send + Sync>>>();

    // Add the default "discard" layer to the collection of layers.
    layers.push(default);

    // Initialize the `tracing_subscriber` registry.
    // The `with` method is used to add multiple layers to the subscriber.
    let subscriber = tracing_subscriber::registry()
        // Add all the provided custom layers (e.g., stream layers).
        .with(layers)
        .with(
            // Configure a file layer specifically for warning and error logs.
            // `with_ansi(false)` disables ANSI escape codes for cleaner file output.
            // `with_writer(error_logs)` directs output to the `error.log` file.
            // `with_filter(LevelFilter::WARN)` ensures only WARN level and above logs are written here.
            fmt::layer()
                .with_ansi(false)
                .with_writer(error_logs)
                .with_filter(LevelFilter::WARN),
        )
        .with(
            // Configure a file layer for general logs (debug, info, warn, error).
            // `with_ansi(false)` for clean file output.
            // `with_writer(logs)` directs output to the `logs.log` file.
            // `with_filter(level)` applies the user-specified minimum log level.
            fmt::layer()
                .with_ansi(false)
                .with_writer(logs)
                .with_filter(level),
        );

    // Conditionally initialize the subscriber with a terminal output layer.
    // This allows users to choose whether to see logs in their console.
    if terminal {
        subscriber
            // Add a terminal layer with default formatting and the specified log level.
            .with(fmt::Layer::default().with_filter(level))
            .init(); // Initialize the global subscriber.
    } else {
        subscriber.init() // Initialize the global subscriber without terminal output.
    }

    Ok(())
}

/// A Python-exposed wrapper around a `tracing_subscriber::Layer`.
/// This struct allows `tracing` layers to be created in Rust and passed
/// around in Python before being used to initialize the tracing subscriber.
///
/// # Why this design?
/// PyO3 requires types to be `#[pyclass]` to be used in Python. Since `tracing_subscriber::Layer`
/// is a trait object (`dyn Layer`), it needs to be wrapped in an `Arc<Box<dyn Layer>>`
/// to be clonable and safely passed across FFI boundaries and shared between threads.
#[pyclass]
#[derive(Clone)]
pub struct StreamLogsLayer {
    // The actual `tracing` layer, wrapped in `Arc` for shared ownership
    // and `Box` for dynamic dispatch.
    layer: Arc<Box<dyn Layer<Registry> + Send + Sync>>,
}

/// A writer implementation that discards all written bytes.
///
/// # Why this design?
/// This `NoneWriter` is used with `fmt::Layer` to create a "noop" or "discard"
/// logging destination. This is useful when you want to ensure that `tracing`
/// doesn't default to printing to `stderr` if no other explicit writer is configured
/// for a layer, or when you want to explicitly disable output for a specific layer.
struct NoneWriter;

impl Write for NoneWriter {
    // Implements the `write` method of the `std::io::Write` trait.
    // It simply returns the length of the buffer, effectively consuming the data
    // without writing it anywhere.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }

    // Implements the `flush` method, which does nothing as no data is buffered.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for NoneWriter {
    type Writer = NoneWriter;
    // Implements `make_writer` to return a new `NoneWriter` instance.
    fn make_writer(&'a self) -> Self::Writer {
        NoneWriter
    }
}

// Type alias for the specific stream type used for logs.
// `Fuse` ensures the stream yields `None` permanently after its first `None`.
// `BoxStream` is a trait object for a stream of `BinaryOptionsResult<String>`.
type LogStream = Fuse<BoxStream<'static, BinaryOptionsResult<String>>>;

/// A Python-exposed iterator for consuming streamed log messages.
/// This allows Python code to asynchronously or synchronously iterate over logs
/// that are being captured by a `StreamLogsLayer`.
///
/// # Why this design?
/// Provides a direct bridge for Python to receive real-time log data from Rust.
/// It wraps an asynchronous stream (`LogStream`) and exposes it through Python's
/// asynchronous (`__anext__`) and synchronous (`__next__`) iterator protocols.
#[pyclass]
pub struct StreamLogsIterator {
    // The underlying log stream, protected by an `Arc<Mutex>` for thread-safe access
    // and shared ownership in an asynchronous context.
    stream: Arc<Mutex<LogStream>>,
}

/// Python methods for `StreamLogsIterator`.
/// Implements Python's iterator and asynchronous iterator protocols.
#[pymethods]
impl StreamLogsIterator {
    /// Implements the `__aiter__` method for asynchronous iteration.
    /// Returns `self` to make the object itself an async iterator.
    fn __aiter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    /// Implements the `__iter__` method for synchronous iteration.
    /// Returns `self` to make the object itself a sync iterator.
    fn __iter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    /// Implements the `__anext__` method for asynchronous iteration.
    /// This method is called by Python's `async for` loop.
    ///
    /// # Why this design?
    /// It uses `future_into_py` to bridge the Rust `async` operation (`next_stream`)
    /// with Python's `asyncio` event loop. This allows Python to `await` the next log entry
    /// without blocking the GIL.
    fn __anext__<'py>(&'py mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stream = self.stream.clone(); // Clone the `Arc` to move into the async block.
        future_into_py(py, next_stream(stream, false)) // Convert the Rust future into a Python awaitable.
    }

    /// Implements the `__next__` method for synchronous iteration.
    /// This method is called by Python's `for` loop.
    ///
    /// # Why this design?
    /// For synchronous iteration, it explicitly blocks the Tokio runtime to wait
    /// for the next log entry. While `__anext__` is preferred for non-blocking I/O,
    /// `__next__` is provided for compatibility with synchronous Python contexts.
    /// It's crucial to use `get_runtime` to ensure the Tokio runtime is properly managed.
    fn __next__<'py>(&'py self, py: Python<'py>) -> PyResult<String> {
        let runtime = get_runtime(py)?; // Get the Tokio runtime.
        let stream = self.stream.clone(); // Clone the `Arc` to move into the async block.
        runtime.block_on(next_stream(stream, true)) // Block the current thread until the next stream item is available.
    }
}

/// A builder pattern struct for configuring and building `tracing` layers.
/// This allows Python users to programmatically construct their logging setup
/// by adding different types of log outputs (stream, file, terminal) before
/// finalizing the configuration.
///
/// # Why this design?
/// The builder pattern provides a fluent and composable API for setting up
/// complex logging configurations. It separates the configuration steps
/// from the final initialization, making the process more organized and less error-prone.
#[pyclass]
#[derive(Default)]
pub struct LogBuilder {
    // A collection of `tracing` layers that will be assembled into the final subscriber.
    layers: Vec<Box<dyn Layer<Registry> + Send + Sync>>,
    // A flag to ensure the `build` method is called only once, preventing re-initialization
    // of the global tracing subscriber, which can lead to issues.
    build: bool,
}

/// Python methods for `LogBuilder`.
#[pymethods]
impl LogBuilder {
    /// The constructor for `LogBuilder` when called from Python.
    #[new]
    pub fn new() -> Self {
        Self::default() // Initializes with default values (empty layers, build flag false).
    }

    /// Creates and adds a `StreamLogsLayer` to the builder, returning a `StreamLogsIterator`.
    /// This allows logs to be streamed back to Python.
    ///
    /// # Arguments
    /// * `level`: The minimum log level for messages to be captured by this stream.
    /// * `timeout`: An optional `chrono::Duration` to specify a timeout for the stream.
    ///
    /// # Why this design?
    /// This method provides a way to create a real-time log feed into Python.
    /// The `StreamLogsIterator` acts as the consumer for this feed.
    #[pyo3(signature = (level = "DEBUG".to_string(), timeout = None))]
    pub fn create_logs_iterator(
        &mut self,
        level: String,
        timeout: Option<Duration>,
    ) -> StreamLogsIterator {
        // Convert `chrono::Duration` to `std::time::Duration` if provided.
        let timeout = match timeout {
            Some(timeout) => match timeout.to_std() {
                Ok(timeout) => Some(timeout),
                Err(e) => {
                    warn!("Error converting duration to std, {e}"); // Log a warning if conversion fails.
                    None
                }
            },
            None => None,
        };
        // Create the stream logging layer and its associated receiver.
        let (layer, inner_iter) =
            stream_logs_layer(level.parse().unwrap_or(Level::DEBUG.into()), timeout);
        // Box and fuse the receiver stream for use in the iterator.
        let stream = RecieverStream::to_stream_static(Arc::new(inner_iter))
            .boxed()
            .fuse();
        // Create the Python-exposed iterator.
        let iter = StreamLogsIterator {
            stream: Arc::new(Mutex::new(stream)),
        };
        // Add the created layer to the builder's list of layers.
        self.layers.push(layer);
        iter
    }

    /// Adds a file logging layer to the builder. Logs will be written to the specified file.
    ///
    /// # Arguments
    /// * `path`: The path to the log file.
    /// * `level`: The minimum log level for messages to be written to this file.
    ///
    /// # Why this design?
    /// File logging is essential for persistent storage of application events,
    /// especially in production environments where console output might not be captured.
    #[pyo3(signature = (path = "logs.log".to_string(), level = "DEBUG".to_string()))]
    pub fn log_file(&mut self, path: String, level: String) -> PyResult<()> {
        // Open the log file, creating it if it doesn't exist and appending to it.
        let logs = OpenOptions::new().append(true).create(true).open(path)?;
        // Create a `fmt::Layer` for file output.
        let layer = fmt::layer()
            .with_ansi(false) // Disable ANSI escape codes for cleaner file output.
            .with_writer(logs) // Direct output to the opened file.
            .with_filter(level.parse().unwrap_or(LevelFilter::DEBUG)) // Apply the specified log level filter.
            .boxed(); // Box the layer for dynamic dispatch.
        self.layers.push(layer); // Add the layer to the builder.
        Ok(())
    }

    /// Adds a terminal (stdout) logging layer to the builder.
    ///
    /// # Arguments
    /// * `level`: The minimum log level for messages to be printed to the terminal.
    ///
    /// # Why this design?
    /// Terminal logging is crucial for development and debugging, providing immediate
    /// feedback on application behavior.
    #[pyo3(signature = (level = "DEBUG".to_string()))]
    pub fn terminal(&mut self, level: String) {
        // Create a default `fmt::Layer` for terminal output.
        let layer = fmt::Layer::default()
            .with_filter(level.parse().unwrap_or(LevelFilter::DEBUG)) // Apply the specified log level filter.
            .boxed(); // Box the layer.
        self.layers.push(layer); // Add the layer to the builder.
    }

    /// Finalizes the logging configuration by building and initializing the global `tracing` subscriber.
    /// This method can only be called once.
    ///
    /// # Why this design?
    /// The `tracing` subscriber is a global singleton. Calling `init()` multiple times can lead
    /// to unexpected behavior or panics. The `build` flag ensures that the setup is performed
    /// only once, enforcing correct usage of the builder pattern.
    pub fn build(&mut self) -> PyResult<()> {
        // Check if the builder has already been used to build the subscriber.
        if self.build {
            return Err(BinaryErrorPy::NotAllowed(
                "Builder has already been built, cannot be called again".to_string(),
            )
            .into()); // Return an error if already built.
        }
        self.build = true; // Mark the builder as built.

        // Add the default "discard" layer. This is added last to ensure that if any
        // log record is not processed by preceding layers, it doesn't fall through
        // to a default `tracing-subscriber` behavior that might print to stderr.
        let default = fmt::Layer::default().with_writer(NoneWriter).boxed();
        self.layers.push(default);

        // Drain the layers from the builder into a new vector.
        // `drain(..)` moves the layers out, leaving the `self.layers` empty.
        let layers = self
            .layers
            .drain(..)
            .collect::<Vec<Box<dyn Layer<Registry> + Send + Sync>>>();

        // Initialize the global `tracing_subscriber` registry with all accumulated layers.
        tracing_subscriber::registry().with(layers).init();
        Ok(())
    }
}

/// A Python-exposed logger struct that provides convenient methods for logging
/// messages at different severity levels (debug, info, warn, error).
///
/// # Why this design?
/// This struct acts as a simple wrapper around `tracing` macros, making them
/// easily callable from Python. It provides a familiar logger interface for Python users.
#[pyclass]
#[derive(Default)]
pub struct Logger;

/// Python methods for `Logger`.
#[pymethods]
impl Logger {
    /// The constructor for `Logger` when called from Python.
    #[new]
    pub fn new() -> Self {
        Self // Initializes an empty Logger struct.
    }

    /// Logs a message at the DEBUG level.
    ///
    /// `#[instrument(target = TARGET, skip(self, message))]` creates a tracing span
    /// for this function call. `target = TARGET` sets the target for the span,
    /// and `skip(self, message)` prevents `self` and `message` from being
    /// included in the span's fields by default (as `message` is explicitly logged).
    ///
    /// # Why this design?
    /// `tracing::instrument` automatically captures context (like function name)
    /// and `tracing::debug!` emits the message, providing rich, structured logs.
    #[instrument(target = TARGET, skip(self, message))]
    pub fn debug(&self, message: String) {
        debug!(message); // Emits a debug-level log record.
    }

    /// Logs a message at the INFO level.
    #[instrument(target = TARGET, skip(self, message))]
    pub fn info(&self, message: String) {
        tracing::info!(message); // Emits an info-level log record.
    }

    /// Logs a message at the WARN level.
    #[instrument(target = TARGET, skip(self, message))]
    pub fn warn(&self, message: String) {
        tracing::warn!(message); // Emits a warning-level log record.
    }

    /// Logs a message at the ERROR level.
    #[instrument(target = TARGET, skip(self, message))]
    pub fn error(&self, message: String) {
        tracing::error!(message); // Emits an error-level log record.
    }
}

// Module for internal tests of the logging system.
#[cfg(test)]
mod tests {
    use std::time::Duration; // Used for durations in test scenarios.

    use futures_util::future::join; // Used to run multiple futures concurrently in tests.
    use serde_json::Value; // Used for parsing JSON log output in tests.
    use tracing::{error, info, trace, warn}; // Tracing macros for emitting test logs.

    use super::*; // Bring all items from the parent module into scope for tests.

    /// Test case for `start_tracing` with basic terminal output.
    #[test]
    fn test_start_tracing() {
        // Initialize tracing with a temporary log path, DEBUG level, terminal output enabled, and no custom layers.
        start_tracing(".".to_string(), "DEBUG".to_string(), true, vec![]).unwrap();

        info!("Test") // Emit an info log to verify terminal output.
    }

    /// Helper function to create a `StreamLogsLayer` and its corresponding `StreamLogsIterator` for tests.
    fn create_logs_iterator_test(level: String) -> (StreamLogsLayer, StreamLogsIterator) {
        // Create the stream layer and its internal receiver.
        let (inner_layer, inner_iter) =
            stream_logs_layer(level.parse().unwrap_or(Level::DEBUG.into()), None);
        // Wrap the layer in `StreamLogsLayer` for PyO3 compatibility.
        let layer = StreamLogsLayer {
            layer: Arc::new(inner_layer),
        };
        // Convert the receiver into a boxed, fused stream.
        let stream = RecieverStream::to_stream_static(Arc::new(inner_iter))
            .boxed()
            .fuse();
        // Create the `StreamLogsIterator`.
        let iter = StreamLogsIterator {
            stream: Arc::new(Mutex::new(stream)),
        };
        (layer, iter) // Return both the layer and the iterator.
    }

    /// Asynchronous test case for `start_tracing` with a stream logging layer.
    /// This test verifies that logs emitted in Rust can be received and processed
    /// by a `StreamLogsIterator`.
    #[tokio::test]
    async fn test_start_tracing_stream() {
        // Create a stream layer that captures logs at ERROR level.
        let (layer, receiver) = create_logs_iterator_test("ERROR".to_string());
        // Initialize tracing with the stream layer, disabling terminal output for this test.
        start_tracing(".".to_string(), "DEBUG".to_string(), false, vec![layer]).unwrap();

        // Asynchronous function that continuously emits logs at different levels.
        async fn log() {
            let mut num = 0;
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await; // Wait for 1 second.
                num += 1;
                trace!(num, "Test trace"); // Trace logs should not be captured by ERROR layer.
                debug!(num, "Test debug"); // Debug logs should not be captured by ERROR layer.
                info!(num, "Test info");   // Info logs should not be captured by ERROR layer.
                warn!(num, "Test warning"); // Warning logs should be captured.
                error!(num, "Test error"); // Error logs should be captured.
            }
        }

        // Asynchronous function that consumes logs from the `StreamLogsIterator`.
        async fn reciever_fn(reciever: StreamLogsIterator) {
            let mut stream = reciever.stream.lock().await; // Acquire a lock on the stream.

            // Iterate over the incoming log messages from the stream.
            while let Some(Ok(value)) = stream.next().await {
                // Parse the received string as JSON to verify its structure.
                let value: Value = serde_json::from_str(&format!("{:?}", value)).unwrap();
                println!("{}", value); // Print the received log (should only be WARN and ERROR).
            }
        }

        // Run both the log emitter and the log receiver concurrently.
        join(log(), reciever_fn(receiver)).await;
    }
}