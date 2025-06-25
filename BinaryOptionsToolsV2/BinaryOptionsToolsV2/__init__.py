# __init__.py
# This file serves as the package initializer for the Python `BinaryOptionsToolsV2` package.
# It defines what objects (classes, functions, submodules) are exposed when the package
# or its submodules are imported, especially when using `from package import *`.
# It also helps in structuring the package by importing and re-exporting components.

# Import all public symbols from the `BinaryOptionsToolsV2` Rust extension module.
# This line makes all classes and functions defined in the PyO3 Rust module directly
# accessible under the `BinaryOptionsToolsV2` Python package namespace.
# `noqa: F403` suppresses the Flake8 warning for 'import *', which is often intentional
# in `__init__.py` files for convenience.
from .BinaryOptionsToolsV2 import *  # noqa: F403

# Optionally, import the documentation string from the Rust module.
# This allows the docstring defined in the Rust `#[pymodule]` to be accessible
# as the package's overall documentation in Python (e.g., `help(BinaryOptionsToolsV2)`).
# `noqa: F401` suppresses the Flake8 warning for 'imported but unused', as it's used for documentation.
from .BinaryOptionsToolsV2 import __doc__  # noqa: F401

# Import the `__all__` list from the `pocketoption` submodule.
# This is a common pattern to re-export names defined in a submodule, making them
# directly accessible from the parent package level.
from .pocketoption import __all__ as __pocket_all__

# Import the `tracing` submodule.
# This makes the `tracing` submodule accessible as `BinaryOptionsToolsV2.tracing`.
from . import tracing

# Import the `validator` submodule.
# This makes the `validator` submodule accessible as `BinaryOptionsToolsV2.validator`.
from . import validator

# Define the `__all__` variable for the `BinaryOptionsToolsV2` package.
# `__all__` is a list of strings defining what symbols are exported when `*` is used
# (e.g., `from BinaryOptionsToolsV2 import *`).
# It combines the re-exported symbols from `pocketoption` with the `tracing` and `validator` submodules,
# ensuring they are part of the public API of the package.
__all__ = __pocket_all__ + ["tracing", "validator"]
