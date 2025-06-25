use std::str;
use std::sync::Arc;
use std::time::Duration;

use binary_options_tools::error::{BinaryOptionsResult, BinaryOptionsToolsError};
use binary_options_tools::pocketoption::error::PocketResult;
use binary_options_tools::pocketoption::pocket_client::PocketOption;
use binary_options_tools::pocketoption::types::base::RawWebsocketMessage;
use binary_options_tools::pocketoption::types::update::DataCandle;
use binary_options_tools::pocketoption::ws::stream::StreamAsset;
use binary_options_tools::reimports::FilteredRecieverStream;
use futures_util::stream::{BoxStream, Fuse};
use futures_util::StreamExt;
use pyo3::{pyclass, pymethods, Bound, IntoPyObjectExt, Py, PyAny, PyResult, Python};
use pyo3_async_runtimes::tokio::future_into_py;
use url::Url;
use uuid::Uuid;

use crate::config::PyConfig;
use crate::error::BinaryErrorPy;
use crate::runtime::get_runtime;
use crate::stream::next_stream;
use crate::validator::RawValidator;
use tokio::sync::Mutex;

#[pyclass]
#[derive(Clone)]
pub struct RawPocketOption {
    client: PocketOption,
}

#[pyclass]
pub struct StreamIterator {
    stream: Arc<Mutex<Fuse<BoxStream<'static, PocketResult<DataCandle>>>>>,
}

#[pyclass]
pub struct RawStreamIterator {
    stream: Arc<Mutex<Fuse<BoxStream<'static, BinaryOptionsResult<RawWebsocketMessage>>>>>,
}

#[pymethods]
impl RawPocketOption {
    #[new]
    #[pyo3(signature = (ssid, config = None))]
    pub fn new(ssid: String, config: Option<PyConfig>, py: Python<'_>) -> PyResult<Self> {
        let runtime = get_runtime(py)?;
        runtime.block_on(async move {
            let client = if let Some(config) = config {
                let builder = config.build()?;
                let config = builder
                    .build()
                    .map_err(BinaryOptionsToolsError::from)
                    .map_err(BinaryErrorPy::from)?;
                PocketOption::new_with_config(ssid, config)
                    .await
                    .map_err(BinaryErrorPy::from)?
            } else {
                PocketOption::new(ssid).await.map_err(BinaryErrorPy::from)?
            };
            Ok(Self { client })
        })
    }

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
            let parsed_url = Url::parse(&url)
                .map_err(|e| BinaryErrorPy::from(BinaryOptionsToolsError::from(e)))?;

            let client = if let Some(config) = config {
                let builder = config.build()?;
                let config = builder
                    .build()
                    .map_err(BinaryOptionsToolsError::from)
                    .map_err(BinaryErrorPy::from)?;
                PocketOption::new_with_config(ssid, config)
                    .await
                    .map_err(BinaryErrorPy::from)?
            } else {
                PocketOption::new_with_url(ssid, parsed_url)
                    .await
                    .map_err(BinaryErrorPy::from)?
            };
            Ok(Self { client })
        })
    }

    pub async fn is_demo(&self) -> bool {
        self.client.is_demo().await
    }

    pub fn buy<'py>(
        &self,
        py: Python<'py>,
        asset: String,
        amount: f64,
        time: u32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        future_into_py(py, async move {
            let res = client
                .buy(asset, amount, time)
                .await
                .map_err(BinaryErrorPy::from)?;
            let deal = serde_json::to_string(&res.1).map_err(BinaryErrorPy::from)?;
            let result = vec![res.0.to_string(), deal];
            Python::with_gil(|py| result.into_py_any(py))
        })
    }

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

    pub fn check_win<'py>(&self, py: Python<'py>, trade_id: String) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        future_into_py(py, async move {
            let res = client
                .check_results(Uuid::parse_str(&trade_id).map_err(BinaryErrorPy::from)?)
                .await
                .map_err(BinaryErrorPy::from)?;
            Python::with_gil(|py| {
                serde_json::to_string(&res)
                    .map_err(BinaryErrorPy::from)?
                    .into_py_any(py)
            })
        })
    }

    pub async fn get_deal_end_time(&self, trade_id: String) -> PyResult<Option<i64>> {
        Ok(self
            .client
            .get_deal_end_time(Uuid::parse_str(&trade_id).map_err(BinaryErrorPy::from)?)
            .await
            .map(|d| d.timestamp()))
    }

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

    pub async fn balance(&self) -> PyResult<String> {
        let res = self.client.get_balance().await;
        Ok(serde_json::to_string(&res).map_err(BinaryErrorPy::from)?)
    }

    pub async fn closed_deals(&self) -> PyResult<String> {
        let res = self.client.get_closed_deals().await;
        Ok(serde_json::to_string(&res).map_err(BinaryErrorPy::from)?)
    }

    pub async fn clear_closed_deals(&self) {
        self.client.clear_closed_deals().await
    }

    pub async fn opened_deals(&self) -> PyResult<String> {
        let res = self.client.get_opened_deals().await;
        Ok(serde_json::to_string(&res).map_err(BinaryErrorPy::from)?)
    }

    pub async fn payout(&self) -> PyResult<String> {
        let res = self.client.get_payout().await;
        Ok(serde_json::to_string(&res).map_err(BinaryErrorPy::from)?)
    }

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

            // Clone the stream_asset and convert it to a BoxStream
            let boxed_stream = StreamAsset::to_stream_static(Arc::new(stream_asset))
                .boxed()
                .fuse();

            // Wrap the BoxStream in an Arc and Mutex
            let stream = Arc::new(Mutex::new(boxed_stream));

            Python::with_gil(|py| StreamIterator { stream }.into_py_any(py))
        })
    }

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
            // Clone the stream_asset and convert it to a BoxStream
            Ok(())
        })
    }

    pub fn create_raw_order<'py>(
        &self,
        py: Python<'py>,
        message: String,
        validator: Bound<'py, RawValidator>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let validator = validator.get().clone();
        future_into_py(py, async move {
            let res = client
                .create_raw_order(message, Box::new(validator))
                .await
                .map_err(BinaryErrorPy::from)?;
            Ok(res.to_string())
        })
    }

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

            // Clone the stream_asset and convert it to a BoxStream
            let boxed_stream = FilteredRecieverStream::to_stream_static(Arc::new(raw_stream))
                .boxed()
                .fuse();

            // Wrap the BoxStream in an Arc and Mutex
            let stream = Arc::new(Mutex::new(boxed_stream));

            Python::with_gil(|py| RawStreamIterator { stream }.into_py_any(py))
        })
    }

    pub fn get_server_time<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        future_into_py(
            py,
            async move { Ok(client.get_server_time().await.timestamp()) },
        )
    }
}

#[pymethods]
impl StreamIterator {
    fn __aiter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __iter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __anext__<'py>(&'py mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stream = self.stream.clone();
        future_into_py(py, async move {
            let res = next_stream(stream, false).await;
            res.map(|res| res.to_string())
        })
    }

    fn __next__<'py>(&'py self, py: Python<'py>) -> PyResult<String> {
        let runtime = get_runtime(py)?;
        let stream = self.stream.clone();
        runtime.block_on(async move {
            let res = next_stream(stream, true).await;
            res.map(|res| res.to_string())
        })
    }
}

#[pymethods]
impl RawStreamIterator {
    fn __aiter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __iter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __anext__<'py>(&'py mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stream = self.stream.clone();
        future_into_py(py, async move {
            let res = next_stream(stream, false).await;
            res.map(|res| res.to_string())
        })
    }

    fn __next__<'py>(&'py self, py: Python<'py>) -> PyResult<String> {
        let runtime = get_runtime(py)?;
        let stream = self.stream.clone();
        runtime.block_on(async move {
            let res = next_stream(stream, true).await;
            res.map(|res| res.to_string())
        })
    }
}
