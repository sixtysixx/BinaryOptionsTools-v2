# pocketoption/syncronous.py
# This file provides a synchronous Python client for interacting with the Pocket Option
# trading platform. It acts as a wrapper around the `PocketOptionAsync` client,
# allowing users to perform trading operations and data retrieval in a blocking,
# synchronous manner, abstracting away the underlying asynchronous nature.
# This is particularly useful for scripts or applications that are not built
# around an `asyncio` event loop.

# Import the asynchronous PocketOptionAsync client, which this synchronous client wraps.
from .asyncronous import PocketOptionAsync

# Import the Config class for managing client configuration settings.
from BinaryOptionsToolsV2.config import Config

# Import the Validator class for defining message validation rules.
from BinaryOptionsToolsV2.validator import Validator

# Import `timedelta` for specifying time durations.
from datetime import timedelta

# Standard Python asynchronous and utility modules.
import asyncio  # Used to manage and run an internal event loop for synchronous execution of async code.
import json  # For handling JSON serialization and deserialization of data.


class SyncSubscription:
    """
    A Python wrapper around a Rust-provided asynchronous stream subscription,
    adapted for synchronous iteration. This class enables seamless synchronous
    iteration over JSON objects received from the Rust backend, automatically
    parsing string messages into Python dictionaries.

    # Why this design?
    The underlying Rust streams (`StreamIterator`, `RawStreamIterator`) are asynchronous
    and yield string data. This wrapper allows these asynchronous streams to be consumed
    in a synchronous Python `for` loop. It internally calls `next()` on the Rust-provided
    synchronous iterator, which blocks until a new item is available.
    JSON parsing is handled automatically, providing structured data to the user.
    """

    def __init__(self, subscription):
        """
        Initializes the SyncSubscription with the raw Rust subscription object.

        Args:
            subscription: The Rust `StreamLogsIterator`, `StreamIterator`, or `RawStreamIterator`
                          instance, which is a synchronous iterator yielding string representations
                          of data (e.g., JSON strings).
        """
        self.subscription = subscription

    def __iter__(self):
        """
        Returns the synchronous iterator for `for` loops.
        This method is required by the synchronous iteration protocol.
        """
        return self

    def __next__(self):
        """
        Retrieves the next item from the wrapped Rust subscription synchronously
        and parses it as a JSON object.

        # Why this design?
        `next(self.subscription)` is the Python built-in function to get the next
        item from a synchronous iterator. This call will block the current thread
        until the Rust side yields a new item. The result, which is a JSON string,
        is then immediately parsed into a Python dictionary using `json.loads()`.
        This provides a blocking, Pythonic way to consume stream data.
        """
        return json.loads(next(self.subscription))


class PocketOption:
    """
    Synchronous client for interacting with the Pocket Option trading platform.

    This class provides a synchronous interface by managing an internal `asyncio`
    event loop. All asynchronous operations of the `PocketOptionAsync` client
    are run on this loop, allowing them to be called from synchronous Python code
    without requiring the user to manage `asyncio` directly.
    """

    def __init__(
        self, ssid: str, url: str | None = None, config: Config | dict | str = None, **_
    ):
        """
        Initializes a new PocketOption instance.

        # Why this design?
        This constructor sets up a dedicated `asyncio` event loop for the instance.
        It then instantiates `PocketOptionAsync` (the asynchronous client) and stores it
        internally. All public methods of this `PocketOption` class will then
        use `self.loop.run_until_complete()` to execute the corresponding
        asynchronous methods of `_client` on this dedicated loop. This pattern
        effectively bridges the asynchronous Rust backend (exposed via `PocketOptionAsync`)
        to a synchronous Python interface.

        Args:
            ssid (str): Session ID for authentication with Pocket Option platform.
            url (str | None, optional): Custom WebSocket server URL. If provided,
                                        this URL takes precedence over any URLs in `config`.
                                        Defaults to None, using the platform's default URL.
            config (Config | dict | str, optional): Configuration options for the client.
                                                    Can be provided as:
                                                    - `Config` object: A direct instance of the `Config` class.
                                                    - `dict`: A dictionary of configuration parameters.
                                                    - `str`: A JSON string containing configuration parameters.
                                                    Configuration parameters are passed to the underlying `PocketOptionAsync`.
            **_: Additional keyword arguments (ignored), allowing for future expansion
                 without breaking existing calls.

        Raises:
            ValueError: If `config` is provided in an unsupported format.
            BinaryErrorPy: If there's an error during the Rust client initialization
                           (e.g., network issues, invalid configuration).
        """
        # Create a new, isolated asyncio event loop for this instance.
        # This prevents interference with other parts of an application that might
        # be using a different or existing event loop.
        self.loop = asyncio.new_event_loop()
        # Instantiate the asynchronous client, passing configuration.
        # The actual connection and setup will happen asynchronously when methods are called.
        self._client = PocketOptionAsync(ssid, url, config)

    def __del__(self):
        """
        Destructor for the PocketOption instance.

        # Why this design?
        It's crucial to properly close the `asyncio` event loop when the `PocketOption`
        instance is no longer needed. This prevents resource leaks and ensures
        a clean shutdown of background asynchronous tasks. `__del__` is called
        automatically when the object is garbage collected.
        """
        # Close the event loop to release resources.
        self.loop.close()

    def buy(
        self, asset: str, amount: float, time: int, check_win: bool = False
    ) -> tuple[str, dict]:
        """
        Places a buy (call) order for the specified asset synchronously.

        # Why this design?
        This method wraps the asynchronous `_client.buy` call. `self.loop.run_until_complete()`
        executes the asynchronous operation on the internal event loop and blocks
        the current thread until the operation completes, providing a synchronous API.

        Args:
            asset (str): Trading asset (e.g., "EURUSD_otc", "EURUSD").
            amount (float): Trade amount in account currency.
            time (int): Expiry time in seconds (e.g., 60 for 1 minute).
            check_win (bool): If True, waits for the trade result after placing the order.
                              Defaults to False.

        Returns:
            tuple[str, dict]: A tuple containing `(trade_id, trade_details)`.
                              - `trade_id` (str): The unique identifier for the trade.
                              - `trade_details` (dict): A dictionary containing trade information.
                                                        If `check_win` is True, it includes the trade result
                                                        ("win"/"loss"/"draw") and profit.
                                                        Otherwise, it includes initial trade parameters.
        """
        return self.loop.run_until_complete(
            self._client.buy(asset, amount, time, check_win)
        )

    def sell(
        self, asset: str, amount: float, time: int, check_win: bool = False
    ) -> tuple[str, dict]:
        """
        Places a sell (put) order for the specified asset synchronously.

        # Why this design?
        Symmetrical to `buy`, this provides a synchronous interface for placing sell orders.

        Args:
            asset (str): Trading asset (e.g., "EURUSD_otc", "EURUSD").
            amount (float): Trade amount in account currency.
            time (int): Expiry time in seconds (e.g., 60 for 1 minute).
            check_win (bool): If True, waits for the trade result after placing the order.
                              Defaults to False.

        Returns:
            tuple[str, dict]: A tuple containing `(trade_id, trade_details)`.
                              - `trade_id` (str): The unique identifier for the trade.
                              - `trade_details` (dict): A dictionary containing trade information.
                                                        If `check_win` is True, it includes the trade result
                                                        ("win"/"loss"/"draw") and profit.
                                                        Otherwise, it includes initial trade parameters.
        """
        return self.loop.run_until_complete(
            self._client.sell(asset, amount, time, check_win)
        )

    def check_win(self, id: str) -> dict:
        """
        Checks the result of a specific trade synchronously.

        # Why this design?
        Wraps the asynchronous `_client.check_win` method to provide a blocking call
        for retrieving trade results.

        Args:
            id (str): ID of the trade to check.

        Returns:
            dict: Trade result containing:
                - `result` (str): "win", "loss", or "draw".
                - `profit` (float): Profit/loss amount.
                - `details` (dict): Additional trade details.
                - `timestamp` (int): Result timestamp.
        """
        return self.loop.run_until_complete(self._client.check_win(id))

    def get_candles(self, asset: str, period: int, offset: int) -> list[dict]:
        """
        Retrieves historical candle data for an asset synchronously.

        # Why this design?
        Provides synchronous access to historical market data by wrapping the
        asynchronous `_client.get_candles` method.

        Args:
            asset (str): Trading asset (e.g., "EURUSD_otc").
            period (int): Candle timeframe in seconds (e.g., 60 for 1-minute candles).
            offset (int): Historical offset in seconds to fetch.

        Returns:
            list[dict]: List of candles, each containing:
                - `time` (int): Candle timestamp.
                - `open` (float): Opening price.
                - `close` (float): Closing price.
                - `high` (float): Highest price.
                - `low` (float): Lowest price.
        """
        return self.loop.run_until_complete(
            self._client.get_candles(asset, period, offset)
        )

    def get_candles_advanced(
        self, asset: str, period: int, offset: int, time: int
    ) -> list[dict]:
        """
        Retrieves historical candle data for an asset from a specific starting time synchronously.

        # Why this design?
        Offers synchronous access to more granular historical data retrieval, wrapping
        `_client.get_candles_advanced`.

        Args:
            asset (str): Trading asset (e.g., "EURUSD_otc").
            period (int): Candle timeframe in seconds (e.g., 60 for 1-minute candles).
            offset (int): Historical offset in seconds to fetch.
            time (int): Unix timestamp to fetch candles from.

        Returns:
            list[dict]: List of candles, each containing:
                - `time` (int): Candle timestamp.
                - `open` (float): Opening price.
                - `high` (float): Highest price.
                - `low` (float): Lowest price.
                - `close` (float): Closing price.
        """
        return self.loop.run_until_complete(
            self._client.get_candles_advanced(asset, period, offset, time)
        )

    def balance(self) -> float:
        """
        Retrieves the current account balance synchronously.

        # Why this design?
        Provides a simple blocking call to get the account balance.

        Returns:
            float: Account balance in account currency.
        """
        return self.loop.run_until_complete(self._client.balance())

    def opened_deals(self) -> list[dict]:
        """
        Returns a list of all currently opened (active) deals as dictionaries synchronously.

        # Why this design?
        Provides synchronous access to active positions.
        """
        return self.loop.run_until_complete(self._client.opened_deals())

    def closed_deals(self) -> list[dict]:
        """
        Returns a list of all closed (completed) deals as dictionaries synchronously.

        # Why this design?
        Provides synchronous access to historical trade records.
        """
        return self.loop.run_until_complete(self._client.closed_deals())

    def clear_closed_deals(self) -> None:
        """
        Removes all the closed deals from the client's internal memory synchronously.
        This function does not return any value.

        # Why this design?
        Provides a synchronous utility to manage the local cache of closed deals.
        """
        self.loop.run_until_complete(self._client.clear_closed_deals())

    def payout(self, asset: None | str | list[str] = None) -> dict | list[str] | int:
        """
        Retrieves current payout percentages for assets synchronously.

        # Why this design?
        Offers synchronous access to payout information with flexible input/output.

        Args:
            asset (str | list[str] | None, optional):
                - If `None` (default), returns a dictionary of all available assets and their payouts.
                - If `str`, returns the payout percentage for that specific asset.
                - If `list[str]`, returns a list of payout percentages for each asset in the same order.

        Returns:
            dict | list[int] | int:
                - `dict`: If `asset` is `None`, e.g., `{"EURUSD_otc": 85, "GBPUSD": 82}`.
                - `list[int]`: If `asset` is a list, e.g., `[85, 82]`.
                - `int`: If `asset` is a string, e.g., `85`.
                - `None`: If a specific `asset` (string) is not found.
        """
        return self.loop.run_until_complete(self._client.payout(asset))

    def history(self, asset: str, period: int) -> list[dict]:
        """
        Returns a list of dictionaries containing the latest historical data available
        for the specified asset, starting from a given `period` synchronously.
        The data format is similar to the `get_candles` function's returned data.

        # Why this design?
        Provides synchronous access to historical data, wrapping `_client.history`.

        Args:
            asset (str): The trading asset (e.g., "EURUSD_otc").
            period (int): The historical period in seconds to fetch data for.

        Returns:
            list[dict]: A list of dictionaries, each representing a historical data point.
        """
        return self.loop.run_until_complete(self._client.history(asset, period))

    def subscribe_symbol(self, asset: str) -> SyncSubscription:
        """
        Creates a real-time data subscription for an asset, returning a synchronous iterator.

        # Why this design?
        This method allows users to consume real-time market data in a blocking `for` loop.
        It calls the internal asynchronous subscription method and wraps the resulting
        Rust iterator in `SyncSubscription` to provide the synchronous Python interface.

        Args:
            asset (str): The trading asset to subscribe to.

        Returns:
            SyncSubscription: A synchronous iterator that yields real-time
                               price updates (candle data) as Python dictionaries.
        """
        return SyncSubscription(
            self.loop.run_until_complete(self._client._subscribe_symbol_inner(asset))
        )

    def subscribe_symbol_chuncked(
        self, asset: str, chunck_size: int
    ) -> SyncSubscription:
        """
        Creates a real-time data subscription for an asset, where data is returned
        in chunks of raw candles, via a synchronous iterator.

        # Why this design?
        Provides synchronous access to chunked real-time data, useful for batch processing
        in synchronous applications.

        Args:
            asset (str): The trading asset to subscribe to.
            chunck_size (int): The number of raw candles to accumulate before yielding a chunk.

        Returns:
            SyncSubscription: A synchronous iterator yielding lists of real-time
                               candle updates, grouped by `chunck_size`.
        """
        return SyncSubscription(
            self.loop.run_until_complete(
                self._client._subscribe_symbol_chuncked_inner(asset, chunck_size)
            )
        )

    def subscribe_symbol_timed(self, asset: str, time: timedelta) -> SyncSubscription:
        """
        Creates a timed real-time data subscription for an asset, returning a synchronous iterator.

        # Why this design?
        Enables synchronous consumption of time-limited real-time data streams.

        Args:
            asset (str): The trading asset to subscribe to.
            time (timedelta): The duration for which the subscription should be active.

        Returns:
            SyncSubscription: A synchronous iterator yielding price updates.
                               The stream will automatically close after the `timedelta` expires.
        """
        return SyncSubscription(
            self.loop.run_until_complete(
                self._client._subscribe_symbol_timed_inner(asset, time)
            )
        )

    def send_raw_message(self, message: str) -> None:
        """
        Sends a raw WebSocket message synchronously without waiting for a response.

        # Why this design?
        Provides a blocking interface for sending raw messages for advanced use cases.

        Args:
            message (str): Raw WebSocket message to send (e.g., '42["ping"]').
        """
        self.loop.run_until_complete(self._client.send_raw_message(message))

    def create_raw_order(self, message: str, validator: Validator) -> str:
        """
        Sends a raw WebSocket message and waits for a validated response synchronously.

        # Why this design?
        Offers synchronous control over sending custom messages and waiting for specific
        responses, useful for integrating with synchronous workflows.

        Args:
            message (str): Raw WebSocket message to send.
            validator (Validator): Validator instance to validate the response.

        Returns:
            str: The first message that matches the validator's conditions.
        """
        return self.loop.run_until_complete(
            self._client.create_raw_order(message, validator)
        )

    def create_raw_order_with_timout(
        self, message: str, validator: Validator, timeout: timedelta
    ) -> str:
        """
        Similar to `create_raw_order` but with a synchronous timeout.

        # Why this design?
        Adds a timeout mechanism to prevent indefinite blocking when waiting for a response.

        Args:
            message (str): Raw WebSocket message to send.
            validator (Validator): Validator instance to validate the response.
            timeout (timedelta): Maximum time to wait for a valid response.

        Returns:
            str: The first message that matches the validator's conditions.

        Raises:
            TimeoutError: If no valid response is received within the timeout period.
        """
        return self.loop.run_until_complete(
            self._client.create_raw_order_with_timout(message, validator, timeout)
        )

    def create_raw_order_with_timeout_and_retry(
        self, message: str, validator: Validator, timeout: timedelta
    ) -> str:
        """
        Similar to `create_raw_order_with_timeout` but with automatic retry on failure synchronously.

        # Why this design?
        Enhances reliability for critical operations in synchronous contexts by providing
        automatic retries.

        Args:
            message (str): Raw WebSocket message to send.
            validator (Validator): Validator instance to validate the response.
            timeout (timedelta): Maximum time to wait for each attempt.

        Returns:
            str: The first message that matches the validator's conditions.
        """
        return self.loop.run_until_complete(
            self._client.create_raw_order_with_timeout_and_retry(
                message, validator, timeout
            )
        )

    def create_raw_iterator(
        self, message: str, validator: Validator, timeout: timedelta | None = None
    ) -> SyncSubscription:
        """
        Creates a synchronous iterator that yields validated WebSocket messages.

        # Why this design?
        Provides synchronous access to filtered raw WebSocket message streams,
        suitable for blocking consumption in traditional Python scripts.

        Args:
            message (str): Initial WebSocket message to send.
            validator (Validator): Validator instance to filter incoming messages.
            timeout (timedelta | None, optional): Optional timeout for the entire stream.

        Returns:
            SyncSubscription: A synchronous iterator yielding validated messages.
        """
        return SyncSubscription(
            self.loop.run_until_complete(
                self._client.create_raw_iterator(message, validator, timeout)
            )
        )

    def get_server_time(self) -> int:
        """
        Returns the current server time as a UNIX timestamp synchronously.

        # Why this design?
        Provides a blocking call to get the server's time for synchronization.

        Returns:
            int: The current server time as a Unix timestamp.
        """
        return self.loop.run_until_complete(self._client.get_server_time())

    def is_demo(self) -> bool:
        """
        Checks if the current account is a demo account synchronously.

        # Why this design?
        Provides a blocking call to determine the account type, useful for
        conditional logic in synchronous applications.

        Returns:
            bool: True if using a demo account, False if using a real account.
        """
        return self.loop.run_until_complete(self._client.is_demo())
