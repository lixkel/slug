use crate::errors::SlugError;
use std::collections::HashMap;

// Unit registry
fn unit_table() -> HashMap<&'static str, f64> {
    let mut t = HashMap::new();

    t.insert("ns", 1.0);
    t.insert("nsec", 1.0);
    t.insert("ns/op", 1.0);
    t.insert("us", 1_000.0);
    t.insert("µs", 1_000.0);
    t.insert("usec", 1_000.0);
    t.insert("us/op", 1_000.0);
    t.insert("µs/op", 1_000.0);
    t.insert("ms", 1_000_000.0);
    t.insert("msec", 1_000_000.0);
    t.insert("ms/op", 1_000_000.0);
    t.insert("s", 1_000_000_000.0);
    t.insert("sec", 1_000_000_000.0);
    t.insert("s/op", 1_000_000_000.0);

    t
}

// Convert raw (value, unit) pair into Slug's base unit (nanoseconds)
// Unknown units fail so the parser author must handle them
pub fn normalize(value: f64, unit: &str) -> Result<f64, SlugError> {
    let table = unit_table();
    let key = unit.trim().to_lowercase();

    let factor = table.get(key.as_str()).ok_or_else(|| SlugError::Parsing(format!("Unknown unit '{}'", unit)))?;

    Ok(value * factor)
}
