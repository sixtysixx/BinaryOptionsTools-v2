// validator.rs
// This file defines a flexible and extensible validation system for raw WebSocket messages,
// designed to be exposed to Python. It allows users to define various validation rules
// (e.g., regex, string matching, logical combinations) and even provide custom Python
// functions for validation.

// Standard library imports.
use std::sync::Arc; // `Arc` for shared ownership, especially for custom Python functions.

// PyO3 imports for exposing Rust functionality to Python.
use pyo3::{
    pyclass,      // Macro to mark a struct as a Python class.
    pymethods,    // Macro to define methods for a Python class.
    types::{PyAnyMethods, PyList}, // `PyList` for handling Python lists of validators.
    Bound,        // Represents a reference to a Python object with a lifetime.
    PyObject,     // A dynamically typed Python object, used for custom Python functions.
    PyResult,     // A Result type for PyO3 operations, converting Rust errors to Python exceptions.
    Python,       // A token representing the Python interpreter's GIL.
};
use regex::Regex; // `Regex` for regular expression matching.

// Internal crate imports.
use crate::error::BinaryResultPy; // Custom Result type for Python compatibility.
// Imports from the `binary_options_tools` crate.
use binary_options_tools::{
    pocketoption::types::base::RawWebsocketMessage, // The raw WebSocket message type to be validated.
    reimports::ValidatorTrait, // The core `ValidatorTrait` that defines the `validate` method.
};

/// `ArrayValidator` is a helper struct used internally by `RawValidator::All` and `RawValidator::Any`.
/// It holds a vector of `RawValidator` instances, allowing for composite validation rules.
#[pyclass] // Marked as `pyclass` because it's used within `RawValidator` which is exposed to Python.
#[derive(Clone)]
pub struct ArrayValidator(Vec<RawValidator>);

/// `BoxedValidator` is a helper struct used internally by `RawValidator::Not`.
/// It holds a `Box`ed `RawValidator`, enabling recursive validation logic.
#[pyclass] // Marked as `pyclass` for similar reasons as `ArrayValidator`.
#[derive(Clone)]
pub struct BoxedValidator(Box<RawValidator>);

/// `RegexValidator` holds a compiled regular expression for validating messages.
#[pyclass]
#[derive(Clone)]
pub struct RegexValidator {
    regex: Regex, // The compiled regular expression.
}

/// `PyCustom` wraps a Python callable object (`PyObject`) that acts as a custom validator.
///
/// # Why this design?
/// This allows Python users to define arbitrary validation logic using Python functions,
/// which can then be seamlessly integrated into the Rust-based validation chain.
/// `Arc<PyObject>` is used for shared ownership and thread safety when passing the Python
/// callable across Rust's asynchronous boundaries.
#[pyclass]
#[derive(Clone)]
pub struct PyCustom {
    custom: Arc<PyObject>, // The Python callable object.
}

/// `RawValidator` is the main Python-exposed enum that defines various types of validation rules.
/// It implements the `ValidatorTrait` to provide a unified `validate` method.
///
/// # Why this design?
/// An enum is used to represent different validation strategies. This provides
/// a clear and type-safe way to define and combine validation rules.
/// `#[pyclass]` makes this enum directly usable as a class in Python, with
/// static methods for constructing different validator types.
#[pyclass]
#[derive(Clone)]
pub enum RawValidator {
    /// No validation is performed; always returns `true`.
    None(),
    /// Validates using a regular expression.
    Regex(RegexValidator),
    /// Checks if the message starts with a specific string.
    StartsWith(String),
    /// Checks if the message ends with a specific string.
    EndsWith(String),
    /// Checks if the message contains a specific substring.
    Contains(String),
    /// Validates if all contained validators return `true`.
    All(ArrayValidator),
    /// Validates if any of the contained validators return `true`.
    Any(ArrayValidator),
    /// Inverts the result of the contained validator.
    Not(BoxedValidator),
    /// Validates using a custom Python function.
    Custom(PyCustom),
}

/// Implementation block for `RawValidator`'s Rust-internal constructors.
/// These methods are typically called by the Python-exposed static methods.
impl RawValidator {
    /// Creates a new `Regex` validator from a pattern string.
    /// Returns a `BinaryResultPy` to propagate `regex::Error` if the pattern is invalid.
    pub fn new_regex(regex: String) -> BinaryResultPy<Self> {
        let regex = Regex::new(®ex)?; // Compile the regex.
        Ok(Self::Regex(RegexValidator { regex }))
    }

    /// Creates an `All` validator from a vector of other `RawValidator`s.
    pub fn new_all(validators: Vec<RawValidator>) -> Self {
        Self::All(ArrayValidator(validators))
    }

    /// Creates an `Any` validator from a vector of other `RawValidator`s.
    pub fn new_any(validators: Vec<RawValidator>) -> Self {
        Self::Any(ArrayValidator(validators))
    }

    /// Creates a `Not` validator that inverts the result of the given validator.
    pub fn new_not(validator: RawValidator) -> Self {
        Self::Not(BoxedValidator(Box::new(validator)))
    }

    /// Creates a `Contains` validator.
    pub fn new_contains(pattern: String) -> Self {
        Self::Contains(pattern)
    }

    /// Creates a `StartsWith` validator.
    pub fn new_starts_with(pattern: String) -> Self {
        Self::StartsWith(pattern)
    }

    /// Creates an `EndsWith` validator.
    pub fn new_ends_with(pattern: String) -> Self {
        Self::EndsWith(pattern)
    }
}

/// Implements the `Default` trait for `RawValidator`, allowing `RawValidator::default()`
/// to create a `None` validator.
impl Default for RawValidator {
    fn default() -> Self {
        Self::None()
    }
}

/// Implements the `ValidatorTrait` for `RawValidator`.
/// This is the core logic that performs the actual validation based on the enum variant.
///
/// # Why this design?
/// This implementation uses a `match` statement to dispatch to the appropriate
/// validation logic based on the `RawValidator` variant. This makes the validation
/// process clear and extensible.
impl ValidatorTrait<RawWebsocketMessage> for RawValidator {
    fn validate(&self, message: &RawWebsocketMessage) -> bool {
        match self {
            Self::None() => true, // Always valid.
            Self::Contains(pat) => message.to_string().contains(pat), // Check if string contains pattern.
            Self::StartsWith(pat) => message.to_string().starts_with(pat), // Check if string starts with pattern.
            Self::EndsWith(pat) => message.to_string().ends_with(pat), // Check if string ends with pattern.
            Self::Not(val) => !val.validate(message), // Invert validation result of inner validator.
            Self::All(val) => val.validate_all(message), // All inner validators must pass.
            Self::Any(val) => val.validate_any(message), // Any inner validator must pass.
            Self::Regex(val) => val.validate(message), // Delegate to RegexValidator.
            Self::Custom(val) => val.validate(message), // Delegate to PyCustom validator.
        }
    }
}

/// Implements the `ValidatorTrait` for `PyCustom`.
/// This is where the Python custom function is actually called.
///
/// # Why this design?
/// This implementation safely calls the stored Python callable from Rust.
/// It acquires the GIL, calls the Python function with the message string,
/// and expects a boolean return value, handling potential Python errors.
impl ValidatorTrait<RawWebsocketMessage> for PyCustom {
    fn validate(&self, message: &RawWebsocketMessage) -> bool {
        Python::with_gil(|py| {
            // Call the Python custom function with the message string as an argument.
            // `expect` is used here because a misconfigured custom function (e.g., not callable)
            // indicates a programming error that should panic in this context.
            let res = self
                .custom
                .call(py, (message.to_string(),), None)
                .expect("Expected provided function to be callable");
            // Extract the boolean result from the Python function's return value.
            // `expect` is used similarly, assuming the Python function returns a boolean.
            res.extract(py)
                .expect("Expected provided function to return a boolean")
        })
    }
}

/// Implementation for `ArrayValidator`'s internal validation logic.
impl ArrayValidator {
    /// Validates if all validators in the array return `true`.
    fn validate_all(&self, message: &RawWebsocketMessage) -> bool {
        self.0.iter().all(|d| d.validate(message))
    }

    /// Validates if any validator in the array returns `true`.
    fn validate_any(&self, message: &RawWebsocketMessage) -> bool {
        self.0.iter().any(|d| d.validate(message))
    }
}

/// Implements the `ValidatorTrait` for `BoxedValidator`.
/// This simply delegates the validation call to the inner boxed validator.
impl ValidatorTrait<RawWebsocketMessage> for BoxedValidator {
    fn validate(&self, message: &RawWebsocketMessage) -> bool {
        self.0.validate(message)
    }
}

/// Implements the `ValidatorTrait` for `RegexValidator`.
/// This performs the actual regular expression match.
impl ValidatorTrait<RawWebsocketMessage> for RegexValidator {
    fn validate(&self, message: &RawWebsocketMessage) -> bool {
        self.regex.is_match(&message.to_string())
    }
}

/// Python methods for `RawValidator`.
/// This block defines the static methods and instance methods callable from Python.
#[pymethods]
impl RawValidator {
    /// The default constructor for `RawValidator` when called from Python,
    /// creating a `None` validator.
    #[new]
    pub fn new() -> Self {
        Self::default()
    }

    /// Static method to create a `Regex` validator.
    ///
    /// # Arguments
    /// * `pattern`: The regex pattern string.
    ///
    /// # Why this design?
    /// Provides a convenient way to construct regex validators directly from Python.
    /// It returns a `PyResult` to handle potential `regex::Error` during pattern compilation.
    #[staticmethod]
    pub fn regex(pattern: String) -> PyResult<Self> {
        Ok(Self::new_regex(pattern)?)
    }

    /// Static method to create a `Contains` validator.
    #[staticmethod]
    pub fn contains(pattern: String) -> Self {
        Self::new_contains(pattern)
    }

    /// Static method to create a `StartsWith` validator.
    #[staticmethod]
    pub fn starts_with(pattern: String) -> Self {
        Self::new_starts_with(pattern)
    }

    /// Static method to create an `EndsWith` validator.
    #[staticmethod]
    pub fn ends_with(pattern: String) -> Self {
        Self::new_ends_with(pattern)
    }

    /// Static method to create a `Not` validator (logical NOT).
    ///
    /// # Arguments
    /// * `validator`: The `RawValidator` instance to negate.
    ///
    /// # Why this design?
    /// Allows for easy inversion of validation rules, enabling complex logical combinations.
    /// `Bound<'_, RawValidator>` is used to safely receive the Python object.
    #[staticmethod]
    pub fn ne(validator: Bound<'_, RawValidator>) -> Self {
        let val = validator.get(); // Get the underlying `RawValidator` enum.
        Self::new_not(val.clone()) // Clone to create a new instance for the `Not` variant.
    }

    /// Static method to create an `All` validator (logical AND).
    ///
    /// # Arguments
    /// * `validator`: A Python `list` of `RawValidator` instances.
    ///
    /// # Why this design?
    /// Enables chaining multiple validation rules where all must pass.
    /// `PyList` is used to receive the list from Python, and `extract` converts it to `Vec<RawValidator>`.
    #[staticmethod]
    pub fn all(validator: Bound<'_, PyList>) -> PyResult<Self> {
        let val = validator.extract::<Vec<RawValidator>>()?; // Extract Python list into Rust Vec.
        Ok(Self::new_all(val))
    }

    /// Static method to create an `Any` validator (logical OR).
    ///
    /// # Arguments
    /// * `validator`: A Python `list` of `RawValidator` instances.
    ///
    /// # Why this design?
    /// Enables flexible validation where at least one rule must pass.
    #[staticmethod]
    pub fn any(validator: Bound<'_, PyList>) -> PyResult<Self> {
        let val = validator.extract::<Vec<RawValidator>>()?;
        Ok(Self::new_any(val))
    }

    /// Static method to create a `Custom` validator from a Python callable.
    ///
    /// # Arguments
    /// * `func`: A Python callable object (e.g., a function, lambda).
    ///
    /// # Why this design?
    /// This is the most powerful feature, allowing users to define any arbitrary
    /// validation logic in Python. The Python callable is wrapped in `PyCustom`
    /// and `Arc` for safe handling in Rust.
    #[staticmethod]
    pub fn custom(func: PyObject) -> Self {
        Self::Custom(PyCustom {
            custom: Arc::new(func),
        })
    }

    /// Instance method to check a given message against the validator.
    ///
    /// # Arguments
    /// * `msg`: The message string to validate.
    ///
    /// # Returns
    /// `true` if the message passes validation, `false` otherwise.
    ///
    /// # Why this design?
    /// Provides a direct way to test a validator instance from Python.
    /// It converts the input string into a `RawWebsocketMessage` and then
    /// calls the internal `validate` method.
    pub fn check(&self, msg: String) -> bool {
        let raw = RawWebsocketMessage::from(msg); // Convert string to `RawWebsocketMessage`.
        self.validate(&raw) // Perform the validation.
    }
}