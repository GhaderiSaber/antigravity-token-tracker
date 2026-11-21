//! CLI module.

pub mod display;
pub mod interactive;

pub use display::*;
pub use interactive::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_progress_bar() {
        assert_eq!(format_progress_bar(100.0), "100.0% [██████████]");
        assert_eq!(format_progress_bar(50.0), "50.0% [█████░░░░░]");
        assert_eq!(format_progress_bar(0.0), "0.0% [░░░░░░░░░░]");
    }
}
