use std::fmt;
use std::error::Error;

// Define the custom error type
#[derive(Debug)]
pub struct NoRepo {
    details: String,
}

// Implement the Error trait for the custom error type
impl Error for NoRepo {
    fn description(&self) -> &str {
        &self.details
    }
}

// Implement the Display trait for the custom error type
impl fmt::Display for NoRepo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

// Optional: Implement a constructor or other methods for convenience
impl NoRepo {
    pub fn new(msg: &str) -> Box<NoRepo> {
        Box::new(NoRepo { details: msg.to_string() })
    }
}
