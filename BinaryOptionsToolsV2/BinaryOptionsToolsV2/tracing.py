# tracing.py
# This file provides a Pythonic interface for the Rust-implemented logging system.
# It wraps the `RustLogger` and `RustLogBuilder` classes from `BinaryOptionsToolsV2`
# and offers helper functions to simplify log configuration and message emission
# for Python users, including handling log streaming and JSON parsing.

# Import the `json` module for parsing structured log messages.
import json

# Import the `start_tracing` function, `Logger` (aliased as `RustLogger`),
# and `LogBuilder` (aliased as `RustLogBuilder`) from the core Rust extension.
from BinaryOptionsToolsV2 import start_tracing
from BinaryOptionsToolsV2 import Logger as RustLogger
from BinaryOptionsToolsV2 import LogBuilder as RustLogBuilder

# Import `timedelta` for specifying time durations for log stream timeouts.
from datetime import timedelta


class LogSubscription:
    """
    A Python wrapper around a Rust log stream subscription, providing
    both asynchronous and synchronous iteration capabilities.

    # Why this design?
    The underlying Rust `StreamLogsIterator` returns raw string messages.
    This Python wrapper automatically parses these strings as JSON,
    making the structured log data directly accessible as Python dictionaries.
    It also implements both `__aiter__`/__anext__` and `__iter__`/__next__`
    to support both async and sync consumption patterns in Python.
    """

    def __init__(self, subscription):
        """
        Initializes the LogSubscription with the raw Rust subscription object.

        Args:
            subscription: The Rust `StreamLogsIterator` instance.
        """
        self.subscription = subscription

    def __aiter__(self):
        """
        Returns the asynchronous iterator for `async for` loops.
        """
        return self

    async def __anext__(self):
        """
        Retrieves the next log message asynchronously and parses it as JSON.

        # Why this design?
        `anext(self.subscription)` is used to await the next item from the
        Rust-provided async iterator. The result, which is a JSON string,
        is then parsed into a Python dictionary.
        """
        return json.loads(await anext(self.subscription))

    def __iter__(self):
        """
        Returns the synchronous iterator for `for` loops.
        """
        return self

    def __next__(self):
        """
        Retrieves the next log message synchronously and parses it as JSON.

        # Why this design?
        `next(self.subscription)` is used to get the next item from the
        Rust-provided sync iterator. This call will block until a message
        is available. The result is then parsed as JSON.
        """
        return json.loads(next(self.subscription))


def start_logs(
    path: str, level: str = "DEBUG", terminal: bool = True, layers: list = None
):
    """
    Initialize the global logging system for the application.

    This function configures the Rust `tracing` subscriber with various
    output destinations (files, terminal, custom layers). It should typically
    be called once at the start of the application.

    Args:
        path (str): The base directory path where log files (e.g., `error.log`, `logs.log`) will be stored.
        level (str): The minimum logging level (e.g., "DEBUG", "INFO", "WARN", "ERROR")
                     for general logs and terminal output. Defaults to "DEBUG".
        terminal (bool): If True, logs will also be displayed in the terminal (stdout).
                         Defaults to True.
        layers (list): An optional list of `BinaryOptionsToolsV2.StreamLogsLayer` objects
                       for additional logging destinations (e.g., streaming logs back to Python).

    Returns:
        None

    Raises:
        Exception: If there's an error starting the logging system (e.g., file permission issues).

    # Why this design?
    This function provides a simple, direct way for Python users to initialize
    the Rust-backed `tracing` system. It wraps the `start_tracing` function
    from the Rust extension, handling default values and basic error reporting.
    It's a convenient entry point for setting up basic logging.
    """
    if layers is None:
        layers = []
    try:
        start_tracing(path, level, terminal, layers)
    except Exception as e:
        # Print a warning if logging initialization fails, but don't re-raise
        # to allow the application to potentially continue without full logging.
        print(f"Error starting logs, {e}")


class Logger:
    """
    A Python logger class wrapping the RustLogger functionality.

    This class provides a familiar logging interface (`debug`, `info`, `warn`, `error`)
    that routes messages to the underlying Rust `tracing` system.

    # Why this design?
    This class acts as a thin wrapper to make the Rust `Logger` accessible and
    idiomatic in Python. It allows Python code to emit structured logs that are
    then processed by the Rust `tracing` subscriber, enabling consistent logging
    across the entire application (Python and Rust parts).
    """

    def __init__(self):
        """
        Initializes the Logger by creating an instance of the RustLogger.
        """
        self.logger = RustLogger()

    def debug(self, message):
        """
        Log a debug message.

        Args:
            message (str): The message to log. It will be converted to string.
        """
        self.logger.debug(str(message))

    def info(self, message):
        """
        Log an informational message.

        Args:
            message (str): The message to log. It will be converted to string.
        """
        self.logger.info(str(message))

    def warn(self, message):
        """
        Log a warning message.

        Args:
            message (str): The message to log. It will be converted to string.
        """
        self.logger.warn(str(message))

    def error(self, message):
        """
        Log an error message.

        Args:
            message (str): The message to log. It will be converted to string.
        """
        self.logger.error(str(message))


class LogBuilder:
    """
    A Python builder class for configuring the logs, creating log layers and iterators.

    This class provides a fluent API for assembling complex logging configurations
    by adding different types of log outputs (file, terminal, stream) before
    finalizing the setup.

    # Why this design?
    The builder pattern allows for a more organized and flexible way to set up
    the `tracing` subscriber, especially when multiple logging destinations
    or custom behaviors are required. It wraps the Rust `LogBuilder` and
    enhances it with Python-specific features like `timedelta` conversion
    and the `LogSubscription` wrapper for stream iterators.
    """

    def __init__(self):
        """
        Initializes the LogBuilder by creating an instance of the RustLogBuilder.
        """
        self.builder = RustLogBuilder()

    def create_logs_iterator(
        self, level: str = "DEBUG", timeout: None | timedelta = None
    ) -> LogSubscription:
        """
        Create a new logs iterator with the specified level and an optional timeout.
        This iterator allows consuming log messages directly in Python.

        Args:
            level (str): The minimum logging level for messages to be captured by this iterator.
                         Defaults to "DEBUG".
            timeout (None | timedelta): An optional `datetime.timedelta` object specifying
                                        how long the log stream should remain active.
                                        If None, the stream remains active indefinitely.

        Returns:
            LogSubscription: A `LogSubscription` instance that provides both
                             asynchronous and synchronous iteration over log messages.

        # Why this design?
        This method is crucial for real-time log analysis or display in Python.
        It converts the Python `timedelta` to a Rust `Duration` and wraps the
        Rust `StreamLogsIterator` in `LogSubscription` to provide JSON parsing
        and dual iteration capabilities.
        """
        return LogSubscription(self.builder.create_logs_iterator(level, timeout))

    def log_file(self, path: str = "logs.log", level: str = "DEBUG"):
        """
        Configure logging to a file.

        Logs at or above the specified `level` will be appended to the file at `path`.

        Args:
            path (str): The path to the log file. Defaults to "logs.log" in the current directory.
            level (str): The minimum log level for this file handler. Defaults to "DEBUG".

        # Why this design?
        Provides a simple way to direct logs to a persistent file, essential for
        post-mortem analysis and auditing. It delegates directly to the Rust `log_file` method.
        """
        self.builder.log_file(path, level)

    def terminal(self, level: str = "DEBUG"):
        """
        Configure logging to the terminal (stdout).

        Logs at or above the specified `level` will be printed to the console.

        Args:
            level (str): The minimum log level for this terminal handler. Defaults to "DEBUG".

        # Why this design?
        Offers immediate visibility into application behavior during development and
        debugging. It delegates directly to the Rust `terminal` method.
        """
        self.builder.terminal(level)

    def build(self):
        """
        Build and initialize the logging configuration. This function should be called only once per execution.

        After this method is called, the global `tracing` subscriber is initialized,
        and further modifications to the `LogBuilder` or attempts to call `build` again
        will result in an error.

        # Why this design?
        This method finalizes the logging setup. It's designed to be called once
        because the `tracing` subscriber is a global singleton. Enforcing single
        initialization prevents potential issues with reconfiguring a live logging system.
        """
        self.builder.build()
