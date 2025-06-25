# config.py
# This file defines a Python-friendly configuration class, `Config`,
# which acts as a wrapper around the Rust-implemented `PyConfig` from the
# `BinaryOptionsToolsV2` library. Its primary purpose is to provide a more
# idiomatic Python interface for managing configuration settings, including
# serialization/deserialization from dictionaries and JSON, and to enforce
# immutability once the configuration is passed to the Rust core.

# Import the Rust-defined PyConfig class from the BinaryOptionsToolsV2 package.
# This is the underlying configuration object that the Rust client expects.
from BinaryOptionsToolsV2 import PyConfig

# Import standard Python typing hints for better readability and type checking.
from typing import Dict, Any, List

# Import `dataclass` for easily creating classes primarily used to store data,
# automatically generating methods like `__init__`, `__repr__`, etc.
from dataclasses import dataclass

# Import the `json` module for JSON serialization and deserialization.
import json


@dataclass
class Config:
    """
    Python wrapper around PyConfig that provides additional functionality
    for configuration management.

    This class simplifies the management of configuration settings for the
    Rust-based trading client. It allows for easy creation, modification,
    and serialization of configuration parameters in a Pythonic way.
    Once the `pyconfig` property is accessed, the configuration is "locked"
    to prevent further modifications, ensuring consistency with the Rust core.
    """

    # Maximum number of allowed loops for certain internal operations in the Rust client.
    # This prevents infinite loops and ensures graceful termination or retries.
    max_allowed_loops: int = 100
    # Sleep interval in milliseconds between operations, to manage rate limits or CPU usage.
    sleep_interval: int = 100
    # Time in seconds to wait before attempting to reconnect to the server after a disconnection.
    reconnect_time: int = 5
    # Timeout in seconds for the initial connection establishment phase.
    connection_initialization_timeout_secs: int = 30
    # General timeout in seconds for various network operations (e.g., sending/receiving data).
    timeout_secs: int = 30
    # A list of WebSocket URLs as strings that the client can attempt to connect to.
    # This allows for specifying multiple endpoints or custom server addresses.
    urls: List[str] = None

    # Extra duration in seconds, used by functions like `check_win` for extended waiting periods.
    # This provides a buffer for operations that might take longer than typical timeouts.
    extra_duration: int = 5

    def __post_init__(self):
        """
        Post-initialization hook for dataclasses.
        Initializes default values and internal state after the primary `__init__` is done.
        """
        # Ensure `urls` is an empty list if not provided, preventing `None` type issues.
        self.urls = self.urls or []
        # Internal PyConfig instance, initialized lazily when `pyconfig` property is accessed.
        self._pyconfig = None
        # A flag indicating if the configuration has been "locked" (i.e., `pyconfig` was accessed).
        # Once locked, the configuration should not be modified to maintain consistency with Rust.
        self._locked = False

    @property
    def pyconfig(self) -> PyConfig:
        """
        Returns the PyConfig instance for use in Rust code.
        Once this is accessed, the configuration becomes locked.

        # Why this design?
        This property provides a controlled way to expose the Rust-compatible `PyConfig` object.
        The lazy initialization ensures `PyConfig` is only created when needed, and the
        `_locked` flag prevents modification of the Python `Config` instance after its
        Rust counterpart has been generated and potentially used by the core client.
        This helps prevent inconsistencies between the Python configuration state and
        the Rust client's active configuration.
        """
        if self._pyconfig is None:
            # Initialize PyConfig if it hasn't been already.
            self._pyconfig = PyConfig()
            # Update the PyConfig instance with current values from this Config object.
            self._update_pyconfig()
        # Lock the configuration after PyConfig has been accessed, preventing further changes.
        self._locked = True
        return self._pyconfig

    def _update_pyconfig(self):
        """
        Updates the internal PyConfig with current values from the Python Config instance.

        # Why this design?
        This internal method is responsible for synchronizing the values from the Python
        `Config` object to the Rust-bound `PyConfig` object. It includes a check for `_locked`
        to ensure that the configuration is not modified once it's in use by the Rust core,
        enforcing immutability for consistency.
        """
        if self._locked:
            # Raise an error if an attempt is made to modify a locked configuration.
            raise RuntimeError(
                "Configuration is locked and cannot be modified after being used"
            )

        if self._pyconfig is None:
            # If `_update_pyconfig` is called directly before `pyconfig` property, initialize `_pyconfig`.
            self._pyconfig = PyConfig()

        # Assign values from the Python `Config` instance to the Rust `PyConfig` instance.
        # This ensures that the Rust-side configuration matches the Python-side.
        self._pyconfig.max_allowed_loops = self.max_allowed_loops
        self._pyconfig.sleep_interval = self.sleep_interval
        self._pyconfig.reconnect_time = self.reconnect_time
        self._pyconfig.connection_initialization_timeout_secs = (
            self.connection_initialization_timeout_secs
        )
        self._pyconfig.timeout_secs = self.timeout_secs
        # Use `.copy()` for lists to prevent mutable state sharing issues.
        self._pyconfig.urls = self.urls.copy()

    @classmethod
    def from_dict(cls, config_dict: Dict[str, Any]) -> "Config":
        """
        Creates a Config instance from a dictionary.

        Args:
            config_dict: Dictionary containing configuration values

        Returns:
            Config instance

        # Why this design?
        This class method provides a convenient way to instantiate `Config` objects
        from a standard Python dictionary. It filters the input dictionary to ensure
        only valid dataclass fields are used, preventing unexpected errors from
        extra keys in the dictionary.
        """
        return cls(
            **{
                k: v
                for k, v in config_dict.items()
                if k
                in Config.__dataclass_fields__  # Filter keys to match dataclass fields.
            }
        )

    @classmethod
    def from_json(cls, json_str: str) -> "Config":
        """
        Creates a Config instance from a JSON string.

        Args:
            json_str: JSON string containing configuration values

        Returns:
            Config instance

        # Why this design?
        This method simplifies loading configuration from JSON strings, which is
        a common format for persistent configuration files or network transfer.
        It leverages `json.loads` and the `from_dict` method for a clean two-step process.
        """
        return cls.from_dict(json.loads(json_str))

    def to_dict(self) -> Dict[str, Any]:
        """
        Converts the configuration to a dictionary.

        Returns:
            Dictionary containing all configuration values

        # Why this design?
        Provides a straightforward way to serialize the `Config` object into a
        standard Python dictionary. This is useful for saving configurations,
        passing them to other Python components, or for debugging.
        """
        return {
            "max_allowed_loops": self.max_allowed_loops,
            "sleep_interval": self.sleep_interval,
            "reconnect_time": self.reconnect_time,
            "connection_initialization_timeout_secs": self.connection_initialization_timeout_secs,
            "timeout_secs": self.timeout_secs,
            "urls": self.urls,
        }

    def to_json(self) -> str:
        """
        Converts the configuration to a JSON string.

        Returns:
            JSON string containing all configuration values

        # Why this design?
        Complements `from_json` by providing an easy way to export the current
        configuration state as a JSON string, suitable for saving to files or
        transmitting over networks. It reuses the `to_dict` method.
        """
        return json.dumps(self.to_dict())

    def update(self, config_dict: Dict[str, Any]) -> None:
        """
        Updates the configuration with values from a dictionary.

        Args:
            config_dict: Dictionary containing new configuration values

        # Why this design?
        Allows for partial updates of the configuration, where only specific
        parameters need to be changed without re-instantiating the entire object.
        The `_locked` check ensures that updates are only permitted before the
        configuration is used by the Rust core, maintaining the immutability policy.
        """
        if self._locked:
            # Prevent updates if the configuration is already locked.
            raise RuntimeError(
                "Configuration is locked and cannot be modified after being used"
            )

        for key, value in config_dict.items():
            # Only update attributes that already exist on the Config instance,
            # preventing arbitrary new attributes from being added.
            if hasattr(self, key):
                setattr(self, key, value)
