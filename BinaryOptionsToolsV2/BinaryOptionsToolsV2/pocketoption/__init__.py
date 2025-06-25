# pocketoption/__init__.py
# This file serves as the package initializer for the Python `pocketoption` submodule.
# It defines what objects (submodules and classes) are exposed when the `pocketoption`
# package is imported, especially when using `from package import *`.
# It aims to provide a clear and organized public API for Pocket Option related functionalities.

"""
Module for Pocket Option related functionality.

Contains asynchronous and synchronous clients,
as well as specific classes for Pocket Option trading.

# Why this design?
This `__init__.py` centralizes the exposure of Pocket Option related components.
By explicitly defining `__all__`, it controls which names are imported when a user
does `from pocketoption import *`, promoting a cleaner namespace and API.
It separates asynchronous and synchronous client implementations into their
respective submodules (`asyncronous` and `syncronous`) for better organization,
while making the main client classes directly accessible from the `pocketoption` package.
"""

# Define the `__all__` variable for the `pocketoption` package.
# This list specifies the names that are exported when `*` is used in an import statement
# (e.g., `from BinaryOptionsToolsV2.pocketoption import *`).
# It includes the submodules `asyncronous` and `syncronous`, and the main client classes
# `PocketOptionAsync` and `PocketOption`, making them part of the public API of this package.
__all__ = ["asyncronous", "syncronous", "PocketOptionAsync", "PocketOption"]

# Import the `asyncronous` submodule.
# This makes the `asyncronous` submodule accessible as `pocketoption.asyncronous`.
from . import asyncronous

# Import the `syncronous` submodule.
# This makes the `syncronous` submodule accessible as `pocketoption.syncronous`.
from . import syncronous

# Re-export the `PocketOptionAsync` class from the `asyncronous` submodule.
# This allows users to import `PocketOptionAsync` directly from `pocketoption`
# (e.g., `from BinaryOptionsToolsV2.pocketoption import PocketOptionAsync`)
# without needing to specify the `asyncronous` submodule.
from .asyncronous import PocketOptionAsync

# Re-export the `PocketOption` class from the `syncronous` submodule.
# This allows users to import `PocketOption` directly from `pocketoption`
# (e.g., `from BinaryOptionsToolsV2.pocketoption import PocketOption`)
# without needing to specify the `syncronous` submodule.
from .syncronous import PocketOption
