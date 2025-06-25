# pocketoption/asyncronous.py
# This file provides the asynchronous Python client for interacting with the Pocket Option
# trading platform. It wraps the core Rust `RawPocketOption` client from `BinaryOptionsToolsV2`,
# exposing its functionalities (like trading, fetching data, and streaming)
# as `async` Python methods. It handles configuration parsing, JSON serialization/deserialization,
# and integrates with Python's `asyncio` for non-blocking operations.

# Import the Validator class for defining message validation rules.
from BinaryOptionsToolsV2.validator import Validator

# Import the Config class for managing client configuration settings.
from BinaryOptionsToolsV2.config import Config

# Import the raw Rust-bound PocketOption client and Logger for internal use.
from BinaryOptionsToolsV2 import RawPocketOption, Logger

# Import `timedelta` for specifying time durations.
from datetime import timedelta

# Standard Python asynchronous and utility modules.
import asyncio  # For asynchronous programming, including `asyncio.timeout`.
import json  # For handling JSON serialization and deserialization of data.
import time  # For time-related operations, like getting current Unix timestamp.
import sys  # For checking Python version for `asyncio.timeout` compatibility.


class AsyncSubscription:
    """
    A Python wrapper around a Rust-provided asynchronous stream subscription.
    This class enables seamless asynchronous iteration over JSON objects received
    from the Rust backend, automatically parsing string messages into Python dictionaries.

    # Why this design?
    The underlying Rust streams (`StreamIterator`, `RawStreamIterator`) yield string data.
    This Python wrapper centralizes the JSON parsing logic, ensuring that consumers
    of these subscriptions always receive ready-to-use Python dictionaries.
    It implements the `__aiter__` and `__anext__` protocols, making it directly compatible
    with Python's `async for` loops, providing an idiomatic asynchronous experience.
    """

    def __init__(self, subscription):
        """
        Initializes the AsyncSubscription with the raw Rust subscription object.

        Args:
            subscription: The Rust `StreamLogsIterator`, `StreamIterator`, or `RawStreamIterator`
                          instance, which is an asynchronous iterator yielding string representations
                          of data (e.g., JSON strings).
        """
        self.subscription = subscription

    def __aiter__(self):
        """
        Returns the asynchronous iterator for `async for` loops.
        This method is required by the asynchronous iteration protocol.
        """
        return self

    async def __anext__(self):
        """
        Retrieves the next item from the wrapped Rust subscription asynchronously
        and parses it as a JSON object.

        # Why this design?
        `anext(self.subscription)` is the Python built-in function to get the next
        item from an asynchronous iterator. This call `await`s the Rust future
        to complete, which yields a string. This string is then immediately
        parsed into a Python dictionary using `json.loads()`.
        This ensures that users always receive structured data.
        """
        return json.loads(await anext(self.subscription))


class PocketOptionAsync:
    """
    Asynchronous client for interacting with the Pocket Option trading platform.

    This class provides a comprehensive set of asynchronous methods for trading,
    fetching market data, managing deals, and subscribing to real-time updates.
    It acts as a high-level Pythonic interface to the underlying Rust client,
    handling data conversions and asynchronous execution.
    """

    def __init__(
        self, ssid: str, url: str | None = None, config: Config | dict | str = None, **_
    ):
        """
        Initializes a new PocketOptionAsync instance.

        # Why this design?
        The constructor is designed to be flexible, accepting configuration in multiple
        formats (Config object, dictionary, JSON string) for ease of use. It prioritizes
        a direct `url` argument over URLs specified in the `config` object.
        It also lazily initializes the `Config` object if not provided, ensuring
        default settings are applied. The `RawPocketOption` (Rust client) is
        initialized here, bridging Python's `__init__` with Rust's asynchronous setup.
        A Python `Logger` instance is also created for internal logging within this class.

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
                                                    Configuration parameters include:
                                                        - `max_allowed_loops` (int): Max event loop iterations.
                                                        - `sleep_interval` (int): Sleep time between operations (ms).
                                                        - `reconnect_time` (int): Time to wait before reconnection attempts (s).
                                                        - `connection_initialization_timeout_secs` (int): Connection initialization timeout (s).
                                                        - `timeout_secs` (int): General operation timeout (s).
                                                        - `urls` (List[str]): List of fallback WebSocket URLs.
            **_: Additional keyword arguments (ignored), allowing for future expansion
                 without breaking existing calls.

        Raises:
            ValueError: If `config` is provided in an unsupported format.
            BinaryErrorPy: If there's an error during the Rust client initialization
                           (e.g., network issues, invalid configuration).
        """
        # Handle different `config` input types.
        if config is not None:
            if isinstance(config, dict):
                self.config = Config.from_dict(config)
            elif isinstance(config, str):
                self.config = Config.from_json(config)
            elif isinstance(config, Config):
                self.config = config
            else:
                raise ValueError(
                    "Config must be either a Config object, dictionary, or JSON string"
                )

            # Initialize the Rust client (`RawPocketOption`) based on whether a custom URL is provided
            # and whether a configuration object is present.
            if url is not None:
                # If a custom URL is given, use `new_with_url` with the custom URL and PyConfig.
                self.client = RawPocketOption.new_with_url(
                    ssid, url, self.config.pyconfig
                )
            else:
                # If no custom URL, use the regular constructor with PyConfig.
                self.client = RawPocketOption(ssid, config, self.config.pyconfig)
        else:
            # If no config is provided, initialize with default Config.
            self.config = Config()
            if url is not None:
                # If a custom URL is given, use `new_with_url` with the custom URL and default PyConfig.
                self.client = RawPocketOption.new_with_url(ssid, url)
            else:
                # Use the default constructor for RawPocketOption.
                self.client = RawPocketOption(ssid)
        # Initialize a Python Logger instance for logging within this class.
        self.logger = Logger()

    async def buy(
        self, asset: str, amount: float, time: int, check_win: bool = False
    ) -> tuple[str, dict]:
        """
        Places a buy (call) order for the specified asset.

        # Why this design?
        This method is `async` to allow non-blocking network requests for placing orders.
        It calls the underlying Rust `self.client.buy` method, which is also asynchronous.
        The `check_win` parameter offers a convenient option to immediately wait for the
        trade result, abstracting away the subsequent `check_win` call.
        JSON parsing is handled internally to return Python dictionaries.

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

        Raises:
            ConnectionError: If connection to the platform fails during the trade.
            ValueError: If invalid parameters are provided to the underlying Rust client.
            TimeoutError: If trade confirmation times out or result check times out.
        """
        # Call the Rust client's asynchronous `buy` method.
        (trade_id, trade_json_str) = await self.client.buy(asset, amount, time)
        if check_win:
            # If `check_win` is True, immediately check the trade result.
            return trade_id, await self.check_win(trade_id)
        else:
            # Otherwise, parse the initial trade details JSON string and return.
            trade = json.loads(trade_json_str)
            return trade_id, trade

    async def sell(
        self, asset: str, amount: float, time: int, check_win: bool = False
    ) -> tuple[str, dict]:
        """
        Places a sell (put) order for the specified asset.

        # Why this design?
        Identical in design to the `buy` method, providing a symmetrical asynchronous
        interface for placing sell orders with optional immediate result checking.

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

        Raises:
            ConnectionError: If connection to the platform fails during the trade.
            ValueError: If invalid parameters are provided to the underlying Rust client.
            TimeoutError: If trade confirmation times out or result check times out.
        """
        (trade_id, trade_json_str) = await self.client.sell(asset, amount, time)
        if check_win:
            return trade_id, await self.check_win(trade_id)
        else:
            trade = json.loads(trade_json_str)
            return trade_id, trade

    async def check_win(self, id: str) -> dict:
        """
        Checks the result of a specific trade by its ID. It intelligently waits
        until the trade's expected end time plus an `extra_duration`.

        # Why this design?
        This method is critical for determining the outcome of a trade.
        It dynamically calculates a timeout based on the trade's end time,
        adding an `extra_duration` (from `Config`) to account for potential
        delays in result availability from the server. It uses `_timeout` to
        ensure the waiting process is non-blocking and handles `TimeoutError`.
        The profit is analyzed to determine "win", "loss", or "draw" status.

        Args:
            id (str): The unique identifier (ID) of the trade to check.

        Returns:
            dict: A dictionary containing the trade result, including:
                - `result` (str): "win", "loss", or "draw".
                - `profit` (float): The profit/loss amount for the trade.
                - `details` (dict): Additional trade details from the platform.
                - `timestamp` (int): The timestamp when the result was obtained.

        Raises:
            ValueError: If `trade_id` is invalid or not found.
            TimeoutError: If the result check times out after waiting.
        """
        # Get the expected end time of the deal from the Rust client.
        end_time = await self.client.get_deal_end_time(id)

        # Calculate the duration to wait until the trade result should be available.
        if end_time is not None:
            # If end_time is available, calculate remaining time.
            duration = end_time - int(time.time())
            if duration <= 0:
                # If the trade has already ended (or time is in the past), wait a minimum of 5 seconds.
                duration = 5
        else:
            # If end_time is not available, default to a 5-second wait.
            duration = 5
        # Add an `extra_duration` from the client's configuration for buffer.
        duration += self.config.extra_duration

        self.logger.debug(f"Timeout set to: {duration} (6 extra seconds)")

        async def check(id_to_check):
            """Internal async helper to repeatedly check trade result."""
            while True:
                trade_json_str = await self.client.check_win(id_to_check)
                trade = json.loads(trade_json_str)
                # Pocket Option typically provides profit/loss.
                # If the trade is still pending, profit might be 0 or null.
                # We assume a non-zero profit indicates a settled trade.
                if trade.get("profit") is not None:
                    win = trade["profit"]
                    if win > 0:
                        trade["result"] = "win"
                    elif win == 0:
                        trade["result"] = "draw"
                    else:
                        trade["result"] = "loss"
                    return trade
                await asyncio.sleep(0.5)  # Wait a bit before retrying if not settled.

        # Use the `_timeout` helper to run the `check` function with a timeout.
        return await _timeout(check(id), duration)

    async def get_candles(self, asset: str, period: int, offset: int) -> list[dict]:
        """
        Retrieves historical candle data for an asset.

        # Why this design?
        Provides asynchronous access to historical market data, which is essential
        for technical analysis and strategy backtesting. It wraps the Rust client's
        `get_candles` method and handles JSON deserialization of the results.

        Args:
            asset (str): Trading asset (e.g., "EURUSD_otc", "EURUSD").
            period (int): The candle timeframe in seconds (e.g., 60 for 1-minute candles, 300 for 5-minute).
            offset (int): The number of candles to offset from the most recent candle.
                          (e.g., 0 for the latest candles, 10 for candles starting 10 periods ago).

        Returns:
            list[dict]: A list of dictionaries, where each dictionary represents a candle
                        and typically contains:
                        - `time` (int): Candle timestamp (Unix timestamp).
                        - `open` (float): Opening price.
                        - `high` (float): Highest price.
                        - `low` (float): Lowest price.
                        - `close` (float): Closing price.

        Note:
            Common `period` values: 1, 5, 15, 30, 60, 300 seconds.
            The maximum number of candles returned depends on the platform's API limits.
        """
        candles_json_str = await self.client.get_candles(asset, period, offset)
        return json.loads(candles_json_str)

    async def get_candles_advanced(
        self, asset: str, period: int, offset: int, time: int
    ) -> list[dict]:
        """
        Retrieves historical candle data for an asset from a specific starting time.

        # Why this design?
        Offers more precise control over historical data retrieval, allowing users
        to specify an exact Unix timestamp from which to start fetching candles.
        This is particularly useful for historical analysis that needs to align
        with specific points in time.

        Args:
            asset (str): Trading asset (e.g., "EURUSD_otc", "EURUSD").
            period (int): The candle timeframe in seconds (e.g., 60 for 1-minute candles).
            offset (int): The number of candles to offset from the specified `time`.
            time (int): A specific Unix timestamp (in seconds) to fetch candles from.

        Returns:
            list[dict]: A list of dictionaries, where each dictionary represents a candle
                        (same format as `get_candles`).

        Note:
            Available `period` values: 1, 5, 15, 30, 60, 300 seconds.
            The actual number of candles returned depends on the platform's API limits
            and the specified `offset`.
        """
        candles_json_str = await self.client.get_candles_advanced(
            asset, period, offset, time
        )
        return json.loads(candles_json_str)

    async def balance(self) -> float:
        """
        Retrieves the current account balance.

        # Why this design?
        Provides a simple, asynchronous way to query the account balance.
        It extracts the 'balance' field from the JSON response for direct use as a float.

        Returns:
            float: The current account balance in the account's currency.

        Note:
            The balance updates in real-time as trades are completed or funds are moved.
        """
        balance_json_str = await self.client.balance()
        return json.loads(balance_json_str)["balance"]

    async def opened_deals(self) -> list[dict]:
        """
        Returns a list of all currently opened (active) deals as dictionaries.

        # Why this design?
        Essential for monitoring active positions. It wraps the Rust client's
        method and deserializes the JSON response into a list of Python dictionaries.
        """
        deals_json_str = await self.client.opened_deals()
        return json.loads(deals_json_str)

    async def closed_deals(self) -> list[dict]:
        """
        Returns a list of all closed (completed) deals as dictionaries.

        # Why this design?
        Provides access to historical trade records for analysis. It wraps the Rust client's
        method and deserializes the JSON response.
        """
        deals_json_str = await self.client.closed_deals()
        return json.loads(deals_json_str)

    async def clear_closed_deals(self) -> None:
        """
        Removes all the closed deals from the client's internal memory.
        This function does not return any value.

        # Why this design?
        Provides a utility to manage the local cache of closed deals,
        useful for applications that need to clear historical data to save memory
        or manage state. It directly calls the Rust client's method.
        """
        await self.client.clear_closed_deals()

    async def payout(
        self, asset: None | str | list[str] = None
    ) -> dict | list[int] | int:
        """
        Retrieves current payout percentages for assets.

        # Why this design?
        This method offers flexibility in fetching payout information:
        it can return all payouts, payouts for a list of specific assets,
        or the payout for a single asset. It handles the JSON deserialization
        and conditional processing based on the `asset` argument type.

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
        payout_json_str = await self.client.payout()
        payout = json.loads(payout_json_str)
        if isinstance(asset, str):
            return payout.get(asset)  # Use .get() to return None if asset not found.
        elif isinstance(asset, list):
            return [payout.get(ast) for ast in asset]  # Return list of payouts.
        return payout  # Return full dictionary if no asset specified.

    async def history(self, asset: str, period: int) -> list[dict]:
        """
        Returns a list of dictionaries containing the latest historical data available
        for the specified asset, starting from a given `period`.
        The data format is similar to the `get_candles` function's returned data.

        # Why this design?
        Provides another way to access historical market data, potentially offering
        different granularity or types of historical records compared to `get_candles`.
        It wraps the Rust client's `history` method and deserializes the JSON response.

        Args:
            asset (str): The trading asset (e.g., "EURUSD_otc").
            period (int): The historical period in seconds to fetch data for.

        Returns:
            list[dict]: A list of dictionaries, each representing a historical data point.
        """
        history_json_str = await self.client.history(asset, period)
        return json.loads(history_json_str)

    # Internal asynchronous methods for subscribing to symbol streams.
    # These are typically wrapped by the public `subscribe_symbol` methods.
    async def _subscribe_symbol_inner(self, asset: str):
        """Internal method to subscribe to a symbol, returning the raw Rust stream iterator."""
        return await self.client.subscribe_symbol(asset)

    async def _subscribe_symbol_chuncked_inner(self, asset: str, chunck_size: int):
        """Internal method to subscribe to a symbol with chunking, returning the raw Rust stream iterator."""
        return await self.client.subscribe_symbol_chuncked(asset, chunck_size)

    async def _subscribe_symbol_timed_inner(self, asset: str, time: timedelta):
        """Internal method to subscribe to a symbol with a time limit, returning the raw Rust stream iterator."""
        return await self.client.subscribe_symbol_timed(asset, time)

    async def subscribe_symbol(self, asset: str) -> AsyncSubscription:
        """
        Creates a real-time data subscription for an asset.

        # Why this design?
        This is a core feature for real-time trading strategies. It establishes
        a WebSocket subscription to receive continuous market data updates.
        The raw Rust stream is wrapped by `AsyncSubscription` to provide
        automatic JSON parsing and Pythonic asynchronous iteration.

        Args:
            asset (str): The trading asset to subscribe to (e.g., "EURUSD_otc").

        Returns:
            AsyncSubscription: An asynchronous iterator that yields real-time
                               price updates (candle data) as Python dictionaries.

        Example:
            ```python
            import asyncio
            from BinaryOptionsToolsV2.pocketoption import PocketOptionAsync

            async def main():
                client = PocketOptionAsync("your_ssid")
                async for update in await client.subscribe_symbol("EURUSD_otc"):
                    print(f"Price update: {update}")
                    # Process the real-time candle data
                    if update['close'] > 1.1000:
                        print("Price is high!")
                    # You can break the loop or handle a specific number of updates
                    # For a continuous stream, the loop runs indefinitely until connection closes or error
            # asyncio.run(main())
            ```
        """
        return AsyncSubscription(await self._subscribe_symbol_inner(asset))

    async def subscribe_symbol_chuncked(
        self, asset: str, chunck_size: int
    ) -> AsyncSubscription:
        """
        Creates a real-time data subscription for an asset, where data is returned
        in chunks of raw candles.

        # Why this design?
        Useful for strategies that process data in batches rather than individual
        candle updates, potentially optimizing performance or simplifying logic.
        The `AsyncSubscription` wrapper ensures JSON parsing and async iteration.

        Args:
            asset (str): The trading asset to subscribe to.
            chunck_size (int): The number of raw candles to accumulate before yielding a chunk.

        Returns:
            AsyncSubscription: An asynchronous iterator yielding lists of real-time
                               candle updates, grouped by `chunck_size`.
        """
        return AsyncSubscription(
            await self._subscribe_symbol_chuncked_inner(asset, chunck_size)
        )

    async def subscribe_symbol_timed(
        self, asset: str, time: timedelta
    ) -> AsyncSubscription:
        """
        Creates a real-time data subscription for an asset with a specified duration.

        # Why this design?
        Allows for time-limited subscriptions, useful for collecting data for a
        specific period or for testing purposes without indefinite streaming.
        The `timedelta` is converted to a Rust `Duration` internally.

        Args:
            asset (str): The trading asset to subscribe to.
            time (timedelta): The duration for which the subscription should be active.

        Returns:
            AsyncSubscription: An asynchronous iterator yielding price updates.
                               The stream will automatically close after the `timedelta` expires.

        Example:
            ```python
            import asyncio
            from datetime import timedelta
            from BinaryOptionsToolsV2.pocketoption import PocketOptionAsync

            async def main():
                client = PocketOptionAsync("your_ssid")
                # Subscribe for 1 minute
                async for update in await client.subscribe_symbol_timed("EURUSD_otc", timedelta(minutes=1)):
                    print(f"Timed update: {update}")
                print("Subscription ended after 1 minute.")
            # asyncio.run(main())
            ```
        """
        return AsyncSubscription(await self._subscribe_symbol_timed_inner(asset, time))

    async def send_raw_message(self, message: str) -> None:
        """
        Sends a raw WebSocket message to the Pocket Option server without waiting for a response.

        # Why this design?
        Provides a low-level "escape hatch" for advanced users who need to send
        custom or undocumented WebSocket messages directly to the server without
        expecting a specific response or validation.

        Args:
            message (str): The raw WebSocket message string to send (e.g., '42["ping"]').
        """
        await self.client.send_raw_message(message)

    async def create_raw_order(self, message: str, validator: Validator) -> str:
        """
        Sends a raw WebSocket message and waits for the first response that matches
        the provided `Validator`'s conditions.

        # Why this design?
        Offers granular control over sending custom messages and waiting for specific
        responses. This is useful for interacting with less common API endpoints or
        implementing custom communication protocols. The `Validator` object ensures
        the received message conforms to expectations.

        Args:
            message (str): The raw WebSocket message string to send.
            validator (Validator): A `Validator` instance used to filter and validate
                                   incoming WebSocket messages. The method will return
                                   the first message that passes this validator.

        Returns:
            str: The raw string content of the first WebSocket message that matches
                 the `validator`'s conditions.

        Example:
            ```python
            import asyncio
            from BinaryOptionsToolsV2.pocketoption import PocketOptionAsync
            from BinaryOptionsToolsV2.validator import Validator

            async def main():
                client = PocketOptionAsync("your_ssid")
                # Define a validator that expects a message starting with '451-["signals/load"'
                validator = Validator.starts_with('451-["signals/load"')
                # Send a raw subscription message and wait for a validated response
                response = await client.create_raw_order(
                    '42["signals/subscribe"]',
                    validator
                )
                print(f"Received validated response: {response}")
            # asyncio.run(main())
            ```
        """
        # Pass the raw Rust validator object to the underlying client.
        return await self.client.create_raw_order(message, validator.raw_validator)

    async def create_raw_order_with_timout(
        self, message: str, validator: Validator, timeout: timedelta
    ) -> str:
        """
        Sends a raw WebSocket message, waits for a validated response, but with a timeout.

        # Why this design?
        Adds a crucial timeout mechanism to `create_raw_order`, preventing the application
        from hanging indefinitely if a response is not received or validated within a
        specified period.

        Args:
            message (str): The raw WebSocket message to send.
            validator (Validator): A `Validator` instance to filter and validate the response.
            timeout (timedelta): The maximum duration to wait for a valid response.

        Returns:
            str: The raw string content of the first message that matches the validator.

        Raises:
            TimeoutError: If no valid response is received within the specified `timeout` period.
        """
        # Pass the raw Rust validator and the timedelta (converted to Rust Duration internally).
        return await self.client.create_raw_order_with_timeout(
            message, validator.raw_validator, timeout
        )

    async def create_raw_order_with_timeout_and_retry(
        self, message: str, validator: Validator, timeout: timedelta
    ) -> str:
        """
        Sends a raw WebSocket message, waits for a validated response with a timeout,
        and automatically retries if an attempt fails or times out.

        # Why this design?
        Enhances the robustness of raw order creation by adding an automatic retry
        mechanism. This is valuable in environments with intermittent network issues
        or when dealing with server responses that might occasionally be delayed.

        Args:
            message (str): The raw WebSocket message to send.
            validator (Validator): A `Validator` instance to filter and validate the response.
            timeout (timedelta): The maximum duration to wait for each individual attempt
                                 (including retries).

        Returns:
            str: The raw string content of the first message that matches the validator.
        """
        # Pass the raw Rust validator and the timedelta (converted to Rust Duration internally).
        return await self.client.create_raw_order_with_timeout_and_retry(
            message, validator.raw_validator, timeout
        )

    async def create_raw_iterator(
        self, message: str, validator: Validator, timeout: timedelta | None = None
    ):
        """
        Creates an asynchronous iterator that yields validated raw WebSocket messages.

        # Why this design?
        Provides advanced users with the ability to create highly customized real-time
        data feeds by sending specific WebSocket commands and applying custom filtering
        and validation logic to the continuous stream of responses. It allows for
        long-running, filtered message consumption.

        Args:
            message (str): The initial WebSocket message to send to start the stream.
            validator (Validator): A `Validator` instance to filter and validate
                                   each incoming message from the stream.
            timeout (timedelta | None, optional): An optional maximum duration for the
                                                  entire stream to remain active. If `None`,
                                                  the stream runs indefinitely.

        Returns:
            AsyncIterator: An asynchronous iterator yielding raw string messages
                           that have passed the `validator`'s conditions.

        Example:
            ```python
            import asyncio
            from datetime import timedelta
            from BinaryOptionsToolsV2.pocketoption import PocketOptionAsync
            from BinaryOptionsToolsV2.validator import Validator

            async def main():
                client = PocketOptionAsync("your_ssid")
                # Validator for messages related to trade confirmation
                trade_confirm_validator = Validator.starts_with('42["trade/confirm"')
                # Create a raw iterator for trade confirmation messages with a 5-minute timeout
                async for message in await client.create_raw_iterator(
                    '42["trade/subscribe"]', # Example: subscribe to trade events
                    trade_confirm_validator,
                    timeout=timedelta(minutes=5)
                ):
                    print(f"Received validated trade message: {message}")
            # asyncio.run(main())
            ```
        """
        # Pass the raw Rust validator and the timedelta (converted to Rust Duration internally).
        return await self.client.create_raw_iterator(
            message, validator.raw_validator, timeout
        )

    async def get_server_time(self) -> int:
        """
        Returns the current server time as a UNIX timestamp (integer seconds since epoch).

        # Why this design?
        Crucial for synchronizing local operations with the server's time, which is
        often necessary for precise trading strategies (e.g., placing trades at the
        start of a new candle). It wraps the Rust client's method.

        Returns:
            int: The current server time as a Unix timestamp.
        """
        return await self.client.get_server_time()

    async def is_demo(self) -> bool:
        """
        Checks if the current account is a demo account.

        # Why this design?
        Provides a quick way to determine the account type, which can be used
        to implement safety checks (e.g., preventing large trades on real accounts)
        or to adjust strategy behavior based on the environment.

        Returns:
            bool: True if using a demo account, False if using a real account.

        Examples:
            ```python
            import asyncio
            from BinaryOptionsToolsV2.pocketoption import PocketOptionAsync

            async def main():
                client = PocketOptionAsync("your_ssid")
                is_demo = await client.is_demo()
                print("Using", "demo" if is_demo else "real", "account")

                balance = await client.balance()
                print(f"{'Demo' if is_demo else 'Real'} account balance: {balance}")

                # Example with trade validation based on account type
                async def safe_trade(asset: str, amount: float, duration: int):
                    is_demo_account = await client.is_demo()
                    if not is_demo_account and amount > 100:
                        print("Warning: Large trade on real account. Consider demo first.")
                        # raise ValueError("Large trades should be tested in demo first")
                    return await client.buy(asset, amount, duration)

                # await safe_trade("EURUSD_otc", 10.0, 60)
                # await safe_trade("EURUSD_otc", 150.0, 60) # This might trigger the warning
            # asyncio.run(main())
            ```
        """
        return await self.client.is_demo()


async def _timeout(future, timeout: int):
    """
    Helper function to apply a timeout to an asynchronous future.
    Uses `asyncio.timeout` for Python 3.11+ and `asyncio.wait_for` for older versions.

    # Why this design?
    This function provides a consistent way to apply timeouts to any `asyncio` future,
    abstracting away the version-specific `asyncio.timeout` vs. `asyncio.wait_for`
    differences. This ensures compatibility across different Python 3 versions.

    Args:
        future: The `asyncio` future or coroutine to run with a timeout.
        timeout (int): The maximum time in seconds to wait for the future to complete.

    Returns:
        The result of the `future` if it completes within the timeout.

    Raises:
        asyncio.TimeoutError: If the `future` does not complete within the `timeout`.
    """
    if sys.version_info[:3] >= (3, 11):
        # Use the newer `asyncio.timeout` context manager for Python 3.11+.
        async with asyncio.timeout(timeout):
            return await future
    else:
        # Use `asyncio.wait_for` for older Python versions.
        return await asyncio.wait_for(future, timeout)
