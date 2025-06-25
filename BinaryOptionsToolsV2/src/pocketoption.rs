// This file defines the Python-facing interface for interacting with the Pocket Option
// trading platform. It wraps the core Rust `PocketOption` client from `binary_options_tools`
// and exposes its functionalities (like trading, fetching data, and streaming) to Python
// using PyO3. It handles the conversion of Rust-specific types and asynchronous operations
// into Python-compatible forms.

// Standard library imports.
use std::str; // Used for string conversions, though not directly in this file's current scope.
use std::sync::Arc; // For shared ownership of data across threads, especially for streams.
use std::time::Duration; // For specifying time durations for timeouts and stream intervals.

// Imports from the `binary_options_tools` crate.
use binary_options_tools::error::{BinaryOptionsResult, BinaryOptionsToolsError}; // Custom error types for the core library.
use binary_options_tools::pocketoption::error::PocketResult; // Result type specific to Pocket Option operations.
use binary_options_tools::pocketoption::pocket_client::PocketOption; // The main Rust client for Pocket Option.
use binary_options_tools::pocketoption::types::base::RawWebsocketMessage; // Represents raw WebSocket messages.
use binary_options_tools::pocketoption::types::update::DataCandle; // Represents market candle data.
use binary_options_tools::pocketoption::ws::stream::StreamAsset; // A stream type for market data.
use binary_options_tools::reimports::FilteredRecieverStream; // A stream type for filtered raw messages.
// Imports for asynchronous stream manipulation.
use futures_util::stream::{
    BoxStream, // A type alias for a trait object representing a stream.
    Fuse,      // Used to make a stream "fuse" (stop yielding items) after it returns `None`.
};
use futures_util::StreamExt; // Provides useful methods for streams, like `boxed()` and `fuse()`.
// PyO3 imports for exposing Rust functionality to Python.
use pyo3::{
    pyclass,         // Macro to mark a struct as a Python class.
    pymethods,       // Macro to define methods for a Python class.
    Bound,           // Represents a reference to a Python object with a lifetime.
    IntoPyObjectExt, // Trait to convert Rust types into Python objects.
    Py,              // A managed pointer to a Python object.
    PyAny,           // A dynamically typed Python object.
    PyResult,        // A Result type for PyO3 operations, converting Rust errors to Python exceptions.
    Python,          // A token representing the Python interpreter's GIL.
};
// `pyo3_async_runtimes` for integrating Rust async functions with Python's asyncio.
use pyo3_async_runtimes::tokio::future_into_py; // Converts a Rust future into a Python awaitable.
use url::Url; // For parsing and validating URLs.
use uuid::Uuid; // For handling universally unique identifiers (e.g., trade IDs).

// Internal crate imports.
use crate::config::PyConfig; // Python-friendly configuration struct.
use crate::error::BinaryErrorPy; // Custom error type for PyO3 compatibility.
use crate::runtime::get_runtime; // Function to get the Tokio runtime instance.
use crate::stream::next_stream; // Helper function to get the next item from a stream.
use crate::validator::RawValidator; // Python-exposed validator for raw messages.
use tokio::sync::Mutex; // An asynchronous mutex for protecting shared data in async contexts.

/// `RawPocketOption` is a Python-exposed class that wraps the core Rust `PocketOption` client.
/// It provides a high-level interface for Python users to interact with the Pocket Option trading platform.
///
/// # Why this design?
/// This wrapper is necessary because the underlying `PocketOption` client uses Rust's asynchronous
/// features and complex data structures that are not directly compatible with Python.
/// `RawPocketOption` acts as a bridge, converting Python calls into Rust async operations
/// and Rust results back into Python objects or exceptions.
#[pyclass]
#[derive(Clone)]
pub struct RawPocketOption {
    client: PocketOption, // The actual Rust Pocket Option client instance.
}

/// `StreamIterator` is a Python-exposed class that allows Python code to iterate
/// over a stream of `DataCandle` objects (market data candles).
/// It implements Python's asynchronous and synchronous iterator protocols.
///
/// # Why this design?
/// Real-time market data is typically streamed. This class provides a Pythonic way
/// to consume that stream, enabling users to build real-time data processing applications.
#[pyclass]
pub struct StreamIterator {
    // The underlying stream of `DataCandle` results, protected by an `Arc<Mutex>` for
    // thread-safe access and shared ownership in an asynchronous context.
    stream: Arc<Mutex<Fuse<BoxStream<'static, PocketResult<DataCandle>>>>>,
}

/// `RawStreamIterator` is a Python-exposed class for iterating over a stream of
/// `RawWebsocketMessage` objects (raw WebSocket messages).
/// It implements Python's asynchronous and synchronous iterator protocols.
///
/// # Why this design?
/// For advanced users or debugging, access to raw WebSocket messages can be crucial.
/// This iterator provides a low-level view of the communication with the trading platform.
#[pyclass]
pub struct RawStreamIterator {
    // The underlying stream of `RawWebsocketMessage` results, protected by `Arc<Mutex>`.
    stream: Arc<Mutex<Fuse<BoxStream<'static, BinaryOptionsResult<RawWebsocketMessage>>>>>,
}

/// Python methods for `RawPocketOption`.
/// This block defines all the methods that will be callable from Python on `RawPocketOption` instances.
#[pymethods]
impl RawPocketOption {
    /// Constructor for `RawPocketOption` when called from Python.
    /// Initializes the Pocket Option client with an SSID and an optional `PyConfig`.
    ///
    /// # Arguments
    /// * `ssid`: The session ID for authentication.
    /// * `config`: An optional `PyConfig` object to customize client settings (e.g., timeouts, URLs).
    /// * `py`: The Python GIL token, required for interacting with the Python interpreter.
    ///
    /// # Why this design?
    /// This method bridges Python's synchronous constructor call with Rust's asynchronous client
    /// initialization (`PocketOption::new` or `new_with_config`). It uses `runtime.block_on`
    /// to synchronously wait for the async client to be ready, which is common for `__new__` methods
    /// in PyO3 when the underlying Rust object requires async setup.
    #[new]
    #[pyo3(signature = (ssid, config = None))]
    pub fn new(ssid: String, config: Option<PyConfig>, py: Python<'_>) -> PyResult<Self> {
        let runtime = get_runtime(py)?; // Get the Tokio runtime to execute async code.
        runtime.block_on(async move {
            // Asynchronously initialize the PocketOption client.
            let client = if let Some(config) = config {
                // If `PyConfig` is provided, build the Rust `Config` from it.
                let builder = config.build()?;
                let config = builder
                    .build() // Build the final `Config` object.
                    .map_err(BinaryOptionsToolsError::from) // Convert errors to `BinaryOptionsToolsError`.
                    .map_err(BinaryErrorPy::from)?; // Convert to Python-compatible error.
                // Create a new PocketOption client with the custom configuration.
                PocketOption::new_with_config(ssid, config)
                    .await
                    .map_err(BinaryErrorPy::from)? // Handle async errors.
            } else {
                // If no `PyConfig` is provided, use the default client initialization.
                PocketOption::new(ssid).await.map_err(BinaryErrorPy::from)?
            };
            Ok(Self { client }) // Return the wrapped client.
        })
    }

    /// Static constructor for `RawPocketOption` that allows specifying a custom WebSocket URL.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `ssid`: The session ID.
    /// * `url`: The custom WebSocket URL string.
    /// * `config`: An optional `PyConfig` for additional customization.
    ///
    /// # Why this design?
    /// Provides flexibility for users who need to connect to non-default Pocket Option endpoints
    /// (e.g., for testing or specific regional servers). It also handles URL parsing and error propagation.
    #[staticmethod]
    #[pyo3(signature = (ssid, url, config = None))]
    pub fn new_with_url(
        py: Python<'_>,
        ssid: String,
        url: String,
        config: Option<PyConfig>,
    ) -> PyResult<Self> {
        let runtime = get_runtime(py)?;
        runtime.block_on(async move {
            // Parse the provided URL string into a `url::Url` object, handling parsing errors.
            let parsed_url = Url::parse(&url)
                .map_err(|e| BinaryErrorPy::from(BinaryOptionsToolsError::from(e)))?;

            let client = if let Some(config) = config {
                // If `PyConfig` is provided, build the Rust `Config` from it.
                let builder = config.build()?;
                let config = builder
                    .build()
                    .map_err(BinaryOptionsToolsError::from)
                    .map_err(BinaryErrorPy::from)?;
                // Create client with custom config.
                PocketOption::new_with_config(ssid, config)
                    .await
                    .map_err(BinaryErrorPy::from)?
            } else {
                // Create client with the specified URL and default config.
                PocketOption::new_with_url(ssid, parsed_url)
                    .await
                    .map_err(BinaryErrorPy::from)?
            };
            Ok(Self { client })
        })
    }

    /// Checks if the connected account is a demo account.
    ///
    /// # Why this design?
    /// Provides a simple synchronous Python method for a quick check,
    /// as the underlying Rust method is `async` but the result is trivial.
    pub async fn is_demo(&self) -> bool {
        self.client.is_demo().await
    }

    /// Executes a "buy" (call) order on the specified asset.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `asset`: The name of the asset (e.g., "EURUSD").
    /// * `amount`: The investment amount.
    /// * `time`: The expiration time in seconds.
    ///
    /// # Why this design?
    /// Trading operations are asynchronous. `future_into_py` is used to expose
    /// the Rust `async` function as a Python awaitable, allowing Python code
    /// to `await` the result without blocking the GIL. The result is converted
    /// to a Python list of strings (trade ID and serialized deal).
    pub fn buy<'py>(
        &self,
        py: Python<'py>,
        asset: String,
        amount: f64,
        time: u32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone(); // Clone the client to move into the async block.
        future_into_py(py, async move {
            let res = client
                .buy(asset, amount, time)
                .await // Await the async buy operation.
                .map_err(BinaryErrorPy::from)?; // Convert Rust error to Python error.
            let deal = serde_json::to_string(&res.1).map_err(BinaryErrorPy::from)?; // Serialize the deal object to JSON string.
            let result = vec![res.0.to_string(), deal]; // Create a vector of strings for Python.
            Python::with_gil(|py| result.into_py_any(py)) // Convert the vector to a Python object.
        })
    }

    /// Executes a "sell" (put) order on the specified asset.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `asset`: The name of the asset.
    /// * `amount`: The investment amount.
    /// * `time`: The expiration time in seconds.
    ///
    /// # Why this design?
    /// Similar to `buy`, this provides an asynchronous interface for selling,
    /// ensuring non-blocking execution in Python.
    pub fn sell<'py>(
        &self,
        py: Python<'py>,
        asset: String,
        amount: f64,
        time: u32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        future_into_py(py, async move {
            let res = client
                .sell(asset, amount, time)
                .await
                .map_err(BinaryErrorPy::from)?;
            let deal = serde_json::to_string(&res.1).map_err(BinaryErrorPy::from)?;
            let result = vec![res.0.to_string(), deal];
            Python::with_gil(|py| result.into_py_any(py))
        })
    }

    /// Checks the results of a previously placed trade.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `trade_id`: The UUID of the trade as a string.
    ///
    /// # Why this design?
    /// Allows Python users to query the outcome of their trades asynchronously.
    /// It handles UUID parsing and JSON serialization of the result.
    pub fn check_win<'py>(&self, py: Python<'py>, trade_id: String) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        future_into_py(py, async move {
            let res = client
                .check_results(Uuid::parse_str(&trade_id).map_err(BinaryErrorPy::from)?) // Parse UUID from string.
                .await
                .map_err(BinaryErrorPy::from)?;
            Python::with_gil(|py| {
                serde_json::to_string(&res) // Serialize the result to a JSON string.
                    .map_err(BinaryErrorPy::from)?
                    .into_py_any(py) // Convert the string to a Python object.
            })
        })
    }

    /// Retrieves the end time of a specific deal.
    ///
    /// # Arguments
    /// * `trade_id`: The UUID of the trade as a string.
    ///
    /// # Why this design?
    /// Provides a synchronous Python method to get deal end times, converting
    /// the Rust `DateTime` object to a Unix timestamp (`i64`) which is commonly
    /// used in Python for time representation.
    pub async fn get_deal_end_time(&self, trade_id: String) -> PyResult<Option<i64>> {
        Ok(self
            .client
            .get_deal_end_time(Uuid::parse_str(&trade_id).map_err(BinaryErrorPy::from)?)
            .await
            .map(|d| d.timestamp())) // Convert `DateTime` to Unix timestamp.
    }

    /// Retrieves historical candle data for a given asset.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `asset`: The name of the asset.
    /// * `period`: The candle period (e.g., 60 for 1-minute candles).
    /// * `offset`: The offset from the current time.
    ///
    /// # Why this design?
    /// Provides asynchronous access to historical market data, crucial for analysis
    /// and strategy development. The result is serialized to JSON for easy consumption in Python.
    pub fn get_candles<'py>(
        &self,
        py: Python<'py>,
        asset: String,
        period: i64,
        offset: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        future_into_py(py, async move {
            let res = client
                .get_candles(asset, period, offset)
                .await
                .map_err(BinaryErrorPy::from)?;
            Python::with_gil(|py| {
                serde_json::to_string(&res)
                    .map_err(BinaryErrorPy::from)?
                    .into_py_any(py)
            })
        })
    }

    /// Retrieves historical candle data with an advanced time parameter.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `asset`: The name of the asset.
    /// * `period`: The candle period.
    /// * `offset`: The offset from the `time` parameter.
    /// * `time`: A specific Unix timestamp to fetch candles from.
    ///
    /// # Why this design?
    /// Offers more granular control over historical data retrieval by allowing a specific
    /// starting time, which is useful for backtesting or specific historical analysis.
    pub fn get_candles_advanced<'py>(
        &self,
        py: Python<'py>,
        asset: String,
        period: i64,
        offset: i64,
        time: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();

        future_into_py(py, async move {
            let res = client
                .get_candles_advanced(asset, period, offset, time)
                .await
                .map_err(BinaryErrorPy::from)?;
            Python::with_gil(|py| {
                serde_json::to_string(&res)
                    .map_err(BinaryErrorPy::from)?
                    .into_py_any(py)
            })
        })
    }

    /// Retrieves the current account balance.
    ///
    /// # Why this design?
    /// Provides a simple asynchronous method to fetch balance information,
    /// returning it as a JSON string for flexible parsing in Python.
    pub async fn balance(&self) -> PyResult<String> {
        let res = self.client.get_balance().await;
        Ok(serde_json::to_string(&res).map_err(BinaryErrorPy::from)?)
    }

    /// Retrieves a list of closed deals (past trades).
    ///
    /// # Why this design?
    /// Allows Python users to review their trading history.
    /// The result is serialized to a JSON string.
    pub async fn closed_deals(&self) -> PyResult<String> {
        let res = self.client.get_closed_deals().await;
        Ok(serde_json::to_string(&res).map_err(BinaryErrorPy::from)?)
    }

    /// Clears the locally cached list of closed deals.
    ///
    /// # Why this design?
    /// Provides a utility to manage local state, especially useful for
    /// applications that process deals and then no longer need to track them.
    pub async fn clear_closed_deals(&self) {
        self.client.clear_closed_deals().await
    }

    /// Retrieves a list of currently opened deals (active trades).
    ///
    /// # Why this design?
    /// Essential for monitoring active positions. The result is serialized to a JSON string.
    pub async fn opened_deals(&self) -> PyResult<String> {
        let res = self.client.get_opened_deals().await;
        Ok(serde_json::to_string(&res).map_err(BinaryErrorPy::from)?)
    }

    /// Retrieves payout information.
    ///
    /// # Why this design?
    /// Provides access to payout details, serialized as a JSON string.
    pub async fn payout(&self) -> PyResult<String> {
        let res = self.client.get_payout().await;
        Ok(serde_json::to_string(&res).map_err(BinaryErrorPy::from)?)
    }

    /// Retrieves trading history for a specific asset and period.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `asset`: The name of the asset.
    /// * `period`: The period for the history (e.g., in seconds).
    ///
    /// # Why this design?
    /// Offers another way to fetch historical data, potentially different in
    /// granularity or format from `get_candles`.
    pub fn history<'py>(
        &self,
        py: Python<'py>,
        asset: String,
        period: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        future_into_py(py, async move {
            let res = client
                .history(asset, period)
                .await
                .map_err(BinaryErrorPy::from)?;
            Python::with_gil(|py| {
                serde_json::to_string(&res)
                    .map_err(BinaryErrorPy::from)?
                    .into_py_any(py)
            })
        })
    }

    /// Subscribes to real-time market data for a given symbol.
    /// Returns a `StreamIterator` for consuming the incoming candle data.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `symbol`: The trading symbol (e.g., "EURUSD").
    ///
    /// # Why this design?
    /// This is a key feature for real-time trading applications. It establishes
    /// a WebSocket subscription and provides a Python-friendly iterator (`StreamIterator`)
    /// to consume the continuous stream of market data.
    pub fn subscribe_symbol<'py>(
        &self,
        py: Python<'py>,
        symbol: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        future_into_py(py, async move {
            let stream_asset = client
                .subscribe_symbol(symbol)
                .await
                .map_err(BinaryErrorPy::from)?;

            // Convert the `StreamAsset` (a receiver-based stream) into a `BoxStream`
            // and then `fuse()` it. `fuse()` makes the stream yield `None` permanently
            // after it first yields `None`, which is good practice for iterators.
            let boxed_stream = StreamAsset::to_stream_static(Arc::new(stream_asset))
                .boxed()
                .fuse();

            // Wrap the `BoxStream` in an `Arc` and `Mutex` for shared, thread-safe access
            // across asynchronous tasks and between Rust and Python.
            let stream = Arc::new(Mutex::new(boxed_stream));

            // Create and return a `StreamIterator` instance, making it available in Python.
            Python::with_gil(|py| StreamIterator { stream }.into_py_any(py))
        })
    }

    /// Subscribes to real-time market data for a given symbol, processing data in chunks.
    /// Returns a `StreamIterator`.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `symbol`: The trading symbol.
    /// * `chunck_size`: The number of data points to group into a single chunk.
    ///
    /// # Why this design?
    /// Useful for applications that prefer to process data in batches rather than
    /// individual events, potentially improving efficiency for certain strategies.
    pub fn subscribe_symbol_chuncked<'py>(
        &self,
        py: Python<'py>,
        symbol: String,
        chunck_size: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        future_into_py(py, async move {
            let stream_asset = client
                .subscribe_symbol_chuncked(symbol, chunck_size)
                .await
                .map_err(BinaryErrorPy::from)?;

            // Clone the stream_asset and convert it to a BoxStream
            let boxed_stream = StreamAsset::to_stream_static(Arc::new(stream_asset))
                .boxed()
                .fuse();

            // Wrap the BoxStream in an Arc and Mutex
            let stream = Arc::new(Mutex::new(boxed_stream));

            Python::with_gil(|py| StreamIterator { stream }.into_py_any(py))
        })
    }

    /// Subscribes to real-time market data for a given symbol, with a time limit.
    /// Returns a `StreamIterator`.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `symbol`: The trading symbol.
    /// * `time`: The duration for which the subscription should be active.
    ///
    /// # Why this design?
    /// Allows for time-limited subscriptions, useful for specific data collection
    /// periods or to prevent indefinite resource usage.
    pub fn subscribe_symbol_timed<'py>(
        &self,
        py: Python<'py>,
        symbol: String,
        time: Duration,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        future_into_py(py, async move {
            let stream_asset = client
                .subscribe_symbol_timed(symbol, time)
                .await
                .map_err(BinaryErrorPy::from)?;

            // Clone the stream_asset and convert it to a BoxStream
            let boxed_stream = StreamAsset::to_stream_static(Arc::new(stream_asset))
                .boxed()
                .fuse();

            // Wrap the BoxStream in an Arc and Mutex
            let stream = Arc::new(Mutex::new(boxed_stream));

            Python::with_gil(|py| StreamIterator { stream }.into_py_any(py))
        })
    }

    /// Sends a raw WebSocket message to the Pocket Option server.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `message`: The raw message string to send.
    ///
    /// # Why this design?
    /// Provides a low-level escape hatch for advanced users who need to send
    /// custom or undocumented WebSocket messages directly to the server.
    pub fn send_raw_message<'py>(
        &self,
        py: Python<'py>,
        message: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        future_into_py(py, async move {
            client
                .send_raw_message(message)
                .await
                .map_err(BinaryErrorPy::from)?;
            Ok(()) // Return Ok(()) if the message was sent successfully.
        })
    }

    /// Creates a raw order using a custom message and a validator.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `message`: The raw order message string.
    /// * `validator`: A `RawValidator` instance for validating the response.
    ///
    /// # Why this design?
    /// Offers maximum flexibility for defining order placement logic, allowing
    /// users to craft specific WebSocket messages and define custom validation
    /// rules for the server's response.
    pub fn create_raw_order<'py>(
        &self,
        py: Python<'py>,
        message: String,
        validator: Bound<'py, RawValidator>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let validator = validator.get().clone(); // Clone the validator to move into the async block.
        future_into_py(py, async move {
            let res = client
                .create_raw_order(message, Box::new(validator)) // Use Box::new for trait object.
                .await
                .map_err(BinaryErrorPy::from)?;
            Ok(res.to_string()) // Return the result as a string.
        })
    }

    /// Creates a raw order with a specified timeout.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `message`: The raw order message string.
    /// * `validator`: A `RawValidator` instance.
    /// * `timeout`: The maximum duration to wait for the order response.
    ///
    /// # Why this design?
    /// Adds a timeout mechanism to raw order placement, preventing indefinite waits
    /// and improving robustness in potentially unreliable network conditions.
    pub fn create_raw_order_with_timeout<'py>(
        &self,
        py: Python<'py>,
        message: String,
        validator: Bound<'py, RawValidator>,
        timeout: Duration,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let validator = validator.get().clone();
        future_into_py(py, async move {
            let res = client
                .create_raw_order_with_timeout(message, Box::new(validator), timeout)
                .await
                .map_err(BinaryErrorPy::from)?;
            Ok(res.to_string())
        })
    }

    /// Creates a raw order with a timeout and automatic retry logic.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `message`: The raw order message string.
    /// * `validator`: A `RawValidator` instance.
    /// * `timeout`: The maximum duration to wait for each attempt.
    ///
    /// # Why this design?
    /// Enhances reliability for critical operations by automatically retrying
    /// failed order attempts within a given timeout, useful for volatile network environments.
    pub fn create_raw_order_with_timeout_and_retry<'py>(
        &self,
        py: Python<'py>,
        message: String,
        validator: Bound<'py, RawValidator>,
        timeout: Duration,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let validator = validator.get().clone();
        future_into_py(py, async move {
            let res = client
                .create_raw_order_with_timeout_and_retry(message, Box::new(validator), timeout)
                .await
                .map_err(BinaryErrorPy::from)?;
            Ok(res.to_string())
        })
    }

    /// Creates a stream of raw WebSocket messages based on a custom message and validator.
    /// Returns a `RawStreamIterator`.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    /// * `message`: The initial message to send to start the stream.
    /// * `validator`: A `RawValidator` instance to filter or validate incoming messages.
    /// * `timeout`: An optional timeout for the stream.
    ///
    /// # Why this design?
    /// Provides advanced users with the ability to create highly customized real-time
    /// data feeds by sending specific WebSocket commands and applying custom filtering
    /// and validation logic to the responses.
    #[pyo3(signature = (message, validator, timeout=None))]
    pub fn create_raw_iterator<'py>(
        &self,
        py: Python<'py>,
        message: String,
        validator: Bound<'py, RawValidator>,
        timeout: Option<Duration>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let validator = validator.get().clone();
        future_into_py(py, async move {
            let raw_stream = client
                .create_raw_iterator(message, Box::new(validator), timeout)
                .await
                .map_err(BinaryErrorPy::from)?;

            // Convert the `FilteredRecieverStream` into a `BoxStream` and `fuse()` it.
            let boxed_stream = FilteredRecieverStream::to_stream_static(Arc::new(raw_stream))
                .boxed()
                .fuse();

            // Wrap the `BoxStream` in an `Arc` and `Mutex` for shared, thread-safe access.
            let stream = Arc::new(Mutex::new(boxed_stream));

            // Create and return a `RawStreamIterator` instance.
            Python::with_gil(|py| RawStreamIterator { stream }.into_py_any(py))
        })
    }

    /// Retrieves the current server time from the Pocket Option API.
    ///
    /// # Arguments
    /// * `py`: The Python GIL token.
    ///
    /// # Why this design?
    /// Provides a way to synchronize local time with the server's time, which is critical
    /// for precise trading operations and data analysis. Returns a Unix timestamp.
    pub fn get_server_time<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        future_into_py(
            py,
            async move { Ok(client.get_server_time().await.timestamp()) }, // Convert `DateTime` to Unix timestamp.
        )
    }
}

/// Python methods for `StreamIterator`.
/// Implements Python's iterator and asynchronous iterator protocols for `DataCandle` streams.
#[pymethods]
impl StreamIterator {
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
    /// with Python's `asyncio` event loop. This allows Python to `await` the next candle
    /// without blocking the GIL. The `DataCandle` is converted to its string representation.
    fn __anext__<'py>(&'py mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stream = self.stream.clone();
        future_into_py(py, async move {
            let res = next_stream(stream, false).await;
            res.map(|res| res.to_string()) // Convert the `DataCandle` to a string.
        })
    }

    /// Implements the `__next__` method for synchronous iteration.
    /// This method is called by Python's `for` loop.
    ///
    /// # Why this design?
    /// For synchronous iteration, it explicitly blocks the Tokio runtime to wait
    /// for the next stream item. While `__anext__` is preferred for non-blocking I/O,
    /// `__next__` is provided for compatibility with synchronous Python contexts.
    fn __next__<'py>(&'py self, py: Python<'py>) -> PyResult<String> {
        let runtime = get_runtime(py)?;
        let stream = self.stream.clone();
        runtime.block_on(async move {
            let res = next_stream(stream, true).await;
            res.map(|res| res.to_string()) // Convert the `DataCandle` to a string.
        })
    }
}

/// Python methods for `RawStreamIterator`.
/// Implements Python's iterator and asynchronous iterator protocols for `RawWebsocketMessage` streams.
#[pymethods]
impl RawStreamIterator {
    /// Implements the `__aiter__` method for asynchronous iteration.
    fn __aiter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    /// Implements the `__iter__` method for synchronous iteration.
    fn __iter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    /// Implements the `__anext__` method for asynchronous iteration.
    ///
    /// # Why this design?
    /// Similar to `StreamIterator::__anext__`, this allows asynchronous consumption
    /// of raw WebSocket messages, converting them to strings for Python.
    fn __anext__<'py>(&'py mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stream = self.stream.clone();
        future_into_py(py, async move {
            let res = next_stream(stream, false).await;
            res.map(|res| res.to_string()) // Convert the `RawWebsocketMessage` to a string.
        })
    }

    /// Implements the `__next__` method for synchronous iteration.
    ///
    /// # Why this design?
    /// Similar to `StreamIterator::__next__`, this allows synchronous consumption
    /// of raw WebSocket messages, blocking the runtime until a message is available.
    fn __next__<'py>(&'py self, py: Python<'py>) -> PyResult<String> {
        let runtime = get_runtime(py)?;
        let stream = self.stream.clone();
        runtime.block_on(async move {
            let res = next_stream(stream, true).await;
            res.map(|res| res.to_string()) // Convert the `RawWebsocketMessage` to a string.
        })
    }
}