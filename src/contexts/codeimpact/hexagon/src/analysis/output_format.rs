/// Output format for analysis reports.
///
/// Pure enum, no serde, no deps. Lives in hexagon per ADR-4.3.
#[derive(Clone, Debug, PartialEq)]
pub enum OutputFormat {
    Console,
    Json,
    Html,
}
