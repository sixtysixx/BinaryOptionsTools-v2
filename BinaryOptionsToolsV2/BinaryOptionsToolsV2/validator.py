# validator.py
# This file provides a high-level, Pythonic wrapper for the Rust-implemented `RawValidator`
# from `BinaryOptionsToolsV2`. It aims to make the message validation functionality
# more accessible and intuitive for Python developers by using familiar class methods
# and clear documentation, while abstracting away the direct interaction with Rust types.

from typing import List
# The `RawValidator` is imported inside the methods to avoid circular dependencies
# at import time, as `__init__.py` imports `validator` and `validator` imports `RawValidator`.


class Validator:
    """
    A high-level wrapper for RawValidator that provides message validation functionality.

    This class provides various methods to validate WebSocket messages using different
    strategies like regex matching, prefix/suffix checking, and logical combinations.
    It simplifies the creation and use of complex validation rules by offering a
    fluent API.

    # Why this design?
    This class serves as an abstraction layer over the raw Rust `RawValidator`.
    It provides a more Pythonic interface, using static methods for construction
    and instance methods for validation, aligning with common Python patterns.
    It also helps manage the import of `RawValidator` lazily to prevent circular
    import issues in the package structure.
    """

    def __init__(self):
        """
        Creates a default validator that accepts all messages (equivalent to `RawValidator.None()`).

        # Why this design?
        Provides a sensible default state for a `Validator` instance,
        allowing it to be instantiated without immediate specific rules.
        The `RawValidator` is imported here to ensure it's available when an instance is created.
        """
        from BinaryOptionsToolsV2 import RawValidator

        self._validator = RawValidator()

    @staticmethod
    def regex(pattern: str) -> "Validator":
        """
        Creates a validator that uses regex pattern matching.

        Args:
            pattern: Regular expression pattern string (e.g., r"^\d+").

        Returns:
            Validator: A new Validator instance that matches messages against the pattern.

        # Why this design?
        Provides a static factory method for creating regex validators,
        making the API clean and intuitive. It wraps the Rust `RawValidator.regex`
        method and returns a Python `Validator` object.
        """
        from BinaryOptionsToolsV2 import RawValidator

        v = Validator()
        v._validator = RawValidator.regex(pattern)
        return v

    @staticmethod
    def starts_with(prefix: str) -> "Validator":
        """
        Creates a validator that checks if messages start with a specific prefix.

        Args:
            prefix: The string that messages should start with.

        Returns:
            Validator: A new Validator instance that matches messages starting with the prefix.

        # Why this design?
        Offers a simple string-based validation rule, useful for common message patterns.
        It wraps the Rust `RawValidator.starts_with` method.
        """
        from BinaryOptionsToolsV2 import RawValidator

        v = Validator()
        v._validator = RawValidator.starts_with(prefix)
        return v

    @staticmethod
    def ends_with(suffix: str) -> "Validator":
        """
        Creates a validator that checks if messages end with a specific suffix.

        Args:
            suffix: The string that messages should end with.

        Returns:
            Validator: A new Validator instance that matches messages ending with the suffix.

        # Why this design?
        Similar to `starts_with`, provides another common string-based validation rule.
        It wraps the Rust `RawValidator.ends_with` method.
        """
        from BinaryOptionsToolsV2 import RawValidator

        v = Validator()
        v._validator = RawValidator.ends_with(suffix)
        return v

    @staticmethod
    def contains(substring: str) -> "Validator":
        """
        Creates a validator that checks if messages contain a specific substring.

        Args:
            substring: The string that should be present in messages.

        Returns:
            Validator: A new Validator instance that matches messages containing the substring.

        # Why this design?
        A versatile string-matching rule, useful for checking the presence of keywords or identifiers.
        It wraps the Rust `RawValidator.contains` method.
        """
        from BinaryOptionsToolsV2 import RawValidator

        v = Validator()
        v._validator = RawValidator.contains(substring)
        return v

    @staticmethod
    def ne(validator: "Validator") -> "Validator":
        """
        Creates a validator that negates another validator's result (logical NOT).

        Args:
            validator: The Validator instance whose result should be negated.

        Returns:
            Validator: A new Validator instance that returns True when the input validator returns False, and vice-versa.

        # Why this design?
        Enables the creation of inverse validation rules (e.g., "does not contain").
        It wraps the Rust `RawValidator.ne` method, passing the underlying Rust `_validator` object.
        """
        from BinaryOptionsToolsV2 import RawValidator

        v = Validator()
        v._validator = RawValidator.ne(validator._validator)
        return v

    @staticmethod
    def all(validators: List["Validator"]) -> "Validator":
        """
        Creates a validator that requires all input validators to match (logical AND).

        Args:
            validators: A list of Validator instances that all must match.

        Returns:
            Validator: A new Validator instance that returns True only if all input validators return True.

        # Why this design?
        Allows for combining multiple validation rules where all conditions must be met.
        It iterates over the Python `Validator` objects to extract their underlying Rust `_validator`
        objects before passing them to the Rust `RawValidator.all` method.
        """
        from BinaryOptionsToolsV2 import RawValidator

        v = Validator()
        # Extract the internal Rust RawValidator from each Python Validator object
        rust_validators = [val._validator for val in validators]
        v._validator = RawValidator.all(rust_validators)
        return v

    @staticmethod
    def any(validators: List["Validator"]) -> "Validator":
        """
        Creates a validator that requires at least one input validator to match (logical OR).

        Args:
            validators: A list of Validator instances where at least one must match.

        Returns:
            Validator: A new Validator instance that returns True if any input validator returns True.

        # Why this design?
        Enables flexible validation where satisfying any one of a set of conditions is sufficient.
        Similar to `all`, it extracts the underlying Rust `_validator` objects.
        """
        from BinaryOptionsToolsV2 import RawValidator

        v = Validator()
        # Extract the internal Rust RawValidator from each Python Validator object
        rust_validators = [val._validator for val in validators]
        v._validator = RawValidator.any(rust_validators)
        return v

    @staticmethod
    def custom(func: callable) -> "Validator":
        """
        Creates a validator that uses a custom Python function for validation.

        IMPORTANT SAFETY AND USAGE NOTES:
        1. The provided function MUST:
            - Take exactly one string parameter (the message to validate).
            - Return a boolean value (True if valid, False otherwise).
            - Be synchronous (not an `async def` function).
        2. If these requirements are not met, the program will crash with a Rust panic
           that CANNOT be caught with Python's `try/except` blocks.
        3. The function will be called directly from Rust, meaning Python's standard
           exception handling mechanisms will not intercept errors occurring within `func`.
        4. Custom validators CANNOT be used in async/threaded contexts due to Python's GIL
           and how PyO3 handles custom callables across async boundaries in this specific setup.

        Args:
            func: A callable that takes a string message and returns a boolean.
                  The function MUST follow the requirements listed above.
                  Returns True if the message is valid, False otherwise.

        Returns:
            Validator: A new Validator instance that uses the custom function for validation.

        Raises:
            Rust panic: If the function doesn't meet the requirements or fails during execution.
                        This cannot be caught with Python exception handling.

        # Why this design?
        This method offers the highest degree of flexibility, allowing users to define
        arbitrary validation logic in pure Python. It wraps the Python callable
        and passes it to the Rust `RawValidator.custom` method. The detailed safety notes
        are crucial for informing users about the strict requirements and limitations
        when bridging Python callables to Rust, especially concerning error handling and concurrency.
        """
        from BinaryOptionsToolsV2 import RawValidator

        v = Validator()
        v._validator = RawValidator.custom(func)
        return v

    def check(self, message: str) -> bool:
        """
        Checks if a message matches this validator's conditions.

        Args:
            message: The string message to validate.

        Returns:
            bool: True if the message matches the validator's conditions, False otherwise.

        # Why this design?
        This is the primary instance method for using a configured validator.
        It delegates the actual validation logic to the underlying Rust `_validator`
        object, providing a clean Python API for validation.
        """
        return self._validator.check(message)

    @property
    def raw_validator(self):
        """
        Returns the underlying RawValidator instance.

        This is mainly used internally by the library but can be useful
        for advanced use cases or when directly interacting with the Rust-bound API.

        # Why this design?
        Provides access to the raw Rust-bound `RawValidator` object. This can be
        useful for debugging or for advanced scenarios where direct interaction
        with the Rust object is necessary, while keeping the primary API Pythonic.
        """
        return self._validator
