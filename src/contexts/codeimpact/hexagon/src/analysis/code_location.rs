use std::fmt;

/// Immutable value object representing a location in source code.
///
/// Validates at construction: line >= 1, col >= 1.
/// Display format: `file:line:col`
#[derive(Clone, Debug, PartialEq)]
pub struct CodeLocation {
    file_path: String,
    line: usize,
    col: usize,
}

impl CodeLocation {
    pub fn new(file_path: String, line: usize, col: usize) -> Self {
        assert!(line >= 1, "line must be >= 1, got {}", line);
        assert!(col >= 1, "col must be >= 1, got {}", col);
        Self {
            file_path,
            line,
            col,
        }
    }

    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    pub fn line(&self) -> usize {
        self.line
    }

    pub fn col(&self) -> usize {
        self.col
    }
}

impl fmt::Display for CodeLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file_path, self.line, self.col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test List:
    // 1. creates with valid line/col — file, line, col accessible
    // 2. display format — file:line:col
    // 3. equality — same values are equal
    // 4. inequality — different values are not equal
    // 5. panic on line == 0
    // 6. panic on col == 0

    #[test]
    fn creates_with_valid_line_and_col() {
        let loc = CodeLocation::new("src/main.rs".into(), 5, 9);
        assert_eq!(loc.file_path(), "src/main.rs");
        assert_eq!(loc.line(), 5);
        assert_eq!(loc.col(), 9);
    }

    #[test]
    fn display_format_file_line_col() {
        let loc = CodeLocation::new("src/main.rs".into(), 5, 9);
        assert_eq!(loc.to_string(), "src/main.rs:5:9");
    }

    #[test]
    fn equality_same_values_are_equal() {
        let a = CodeLocation::new("src/main.rs".into(), 5, 9);
        let b = CodeLocation::new("src/main.rs".into(), 5, 9);
        assert_eq!(a, b);
    }

    #[test]
    fn inequality_different_values_not_equal() {
        let a = CodeLocation::new("src/main.rs".into(), 5, 9);
        let b = CodeLocation::new("src/main.rs".into(), 5, 10);
        assert_ne!(a, b);
    }

    #[test]
    #[should_panic(expected = "line must be >= 1")]
    fn panics_on_line_zero() {
        CodeLocation::new("src/main.rs".into(), 0, 1);
    }

    #[test]
    #[should_panic(expected = "col must be >= 1")]
    fn panics_on_col_zero() {
        CodeLocation::new("src/main.rs".into(), 1, 0);
    }
}