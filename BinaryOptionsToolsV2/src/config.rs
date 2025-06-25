// This file defines the `PyConfig` struct, which serves as a Python-friendly configuration object
// for the binary options trading tools. It allows Python users to set various parameters
// for the underlying Rust-based trading client, such as connection timeouts, sleep intervals,
// and WebSocket URLs. The `PyConfig` struct can then be used to build a `ConfigBuilder`
// for the `binary_options_tools` library.

// Import necessary types from the `binary_options_tools` crate.
// `PocketData` is likely a data structure representing market data or similar information
// specific to the Pocket Option platform.
use binary_options_tools::pocketoption::types::data::PocketData;
// `ConfigBuilder` is a builder pattern struct used to construct configuration objects
// for the trading client.
use binary_options_tools::reimports::ConfigBuilder;
// `BinaryOptionsToolsError` is the custom error type for the `binary_options_tools` crate,
// used for error handling within the build process.
use binary_options_tools::{
    error::BinaryOptionsToolsError,
    // `WebSocketMessage` is a type representing messages exchanged over WebSocket connections,
    // likely specific to the Pocket Option API.
    pocketoption::parser::message::WebSocketMessage,
};
// `pyo3::prelude::*` brings in common PyO3w traits and macros required for
// exposing Rust types and functions to Python.
use pyo3::prelude::*;
// `std::collections::HashSet` is used to store unique URLs for the connection,
// ensuring no duplicate URLs are attempted.
use std::collections::HashSet;
// `std::time::Duration` is used to specify time durations for timeouts and intervals.
use std::time::Duration;
// `url::Url` is used for parsing and validating URL strings, ensuring that
// connection URLs are correctly formatted.
use url::Url;

// Import the custom error type `BinaryResultPy` from the local crate's error module.
// This is likely a `Result` type aliased for Python compatibility, typically
// converting Rust errors into Python exceptions.
use crate::error::BinaryResultPy;

/// `PyConfig` represents the configuration settings for the binary options trading client,
/// designed to be exposed and manipulated from Python.
///
/// It uses `#[pyclass]` to make the struct accessible from Python as a class.
/// `#[derive(Clone, Default)]` allows easy duplication and default initialization of the struct.
#[pyclass]
#[derive(Clone, Default)]
pub struct PyConfig {
    /// `max_allowed_loops` defines the maximum number of times a certain operation (e.g.,
    /// a connection attempt loop or a message processing loop) is allowed to repeat.
    /// This prevents infinite loops and ensures the application can gracefully exit or retry.
    #[pyo3(get, set)] // Allows Python to get and set this field.
    pub max_allowed_loops: u32,
    /// `sleep_interval` specifies the duration (in milliseconds) to pause execution
    /// between operations, typically to prevent overwhelming a server or to reduce CPU usage.
    #[pyo3(get, set)]
    pub sleep_interval: u64,
    /// `reconnect_time` defines the delay (in seconds) before attempting to reconnect
    /// to the WebSocket server after a disconnection. This helps in managing network instability.
    #[pyo3(get, set)]
    pub reconnect_time: u64,
    /// `connection_initialization_timeout_secs` sets the maximum time (in seconds) allowed
    /// for the initial establishment of a WebSocket connection. If the connection isn't
    /// established within this period, it's considered a failure.
    #[pyo3(get, set)]
    pub connection_initialization_timeout_secs: u64,
    /// `timeout_secs` specifies a general timeout (in seconds) for various network operations,
    /// such as sending or receiving data. This prevents operations from hanging indefinitely.
    #[pyo3(get, set)]
    pub timeout_secs: u64,
    /// `urls` is a list of WebSocket URLs (as strings) that the client can attempt to connect to.
    /// The client might try these URLs in order or use them for fallback.
    #[pyo3(get, set)]
    pub urls: Vec<String>,
}

/// Implementation block for `PyConfig` methods that are exposed to Python.
#[pymethods]
impl PyConfig {
    /// `new` is the constructor for the `PyConfig` class when called from Python.
    /// It initializes the configuration with sensible default values, providing
    /// a baseline for users who don't want to specify every parameter.
    #[new] // Marks this as the Python constructor.
    pub fn new() -> Self {
        Self {
            max_allowed_loops: 100, // Default to 100 loop iterations.
            sleep_interval: 100,    // Default to 100 ms sleep.
            reconnect_time: 5,      // Default to 5 seconds before reconnecting.
            connection_initialization_timeout_secs: 30, // Default to 30 seconds for initial connection.
            timeout_secs: 30,       // Default to 30 seconds for general operations.
            urls: Vec::new(),       // Start with an empty list of URLs.
        }
    }
}

/// Implementation block for `PyConfig` methods that are not directly exposed to Python,
/// but are used internally, such as building the `ConfigBuilder`.
impl PyConfig {
    /// `build` converts the Python-friendly `PyConfig` into a Rust-native `ConfigBuilder`.
    /// This method is crucial because the `binary_options_tools` library expects a
    /// `ConfigBuilder` instance for its operations, not the `PyConfig` itself.
    ///
    /// It parses the string URLs into `url::Url` objects and handles any parsing errors.
    /// It then populates the `ConfigBuilder` with the values set in `PyConfig`.
    ///
    /// Returns `BinaryResultPy<ConfigBuilder<PocketData, WebSocketMessage, ()>>` which is a
    /// `Result` type that can either contain the successfully built `ConfigBuilder` or an error,
    /// compatible with Python error handling.
    pub fn build(&self) -> BinaryResultPy<ConfigBuilder<PocketData, WebSocketMessage, ()>> {
        // Attempt to parse all URL strings from `self.urls` into `url::Url` objects.
        // `collect()` on the iterator will return a `Result<Vec<Url>, url::ParseError>`,
        // indicating if all URLs were parsed successfully or if any failed.
        let urls: Result<Vec<Url>, url::ParseError> =
            self.urls.iter().map(|url| Url::parse(url)).collect();

        // Initialize a new `ConfigBuilder` and chain its methods to set the configuration parameters.
        // `max_allowed_loops`, `sleep_interval`, `reconnect_time`, and `timeout` are set directly.
        // `default_connection_url` expects a `HashSet` of `Url` objects, so the parsed URLs
        // are converted into a `HashSet`.
        // `urls.map_err(|e| BinaryOptionsToolsError::from(e))?` handles potential URL parsing errors:
        // if `urls` is an `Err`, it maps the `url::ParseError` into a `BinaryOptionsToolsError`
        // and propagates the error using `?`. If it's `Ok`, the `Vec<Url>` is unwrapped.
        let config = ConfigBuilder::new()
            .max_allowed_loops(self.max_allowed_loops)
            .sleep_interval(self.sleep_interval)
            .reconnect_time(self.reconnect_time)
            .timeout(Duration::from_secs(self.timeout_secs))
            .default_connection_url(HashSet::from_iter(
                urls.map_err(|e| BinaryOptionsToolsError::from(e))?,
            ));
        // Return the successfully built `ConfigBuilder` wrapped in an `Ok` variant.
        Ok(config)
    }
}