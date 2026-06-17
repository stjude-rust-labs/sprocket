use wdl_grammar::Diagnostic;

#[test]
fn test_diagnostic_help_getter_with_value() {
    let diagnostic = Diagnostic::error("test error").with_help("This is helpful");

    assert_eq!(diagnostic.help(), Some("This is helpful"));
}

#[test]
fn test_diagnostic_help_getter_without_value() {
    let diagnostic = Diagnostic::error("test error");

    assert_eq!(diagnostic.help(), None);
}

#[test]
fn test_help_getter_consistency() {
    let diagnostic = Diagnostic::warning("test warning")
        .with_help("help message")
        .with_fix("fix message")
        .with_rule("TestRule");

    // Verify all getters work together
    assert_eq!(diagnostic.help(), Some("help message"));
    assert_eq!(diagnostic.fix(), Some("fix message"));
    assert_eq!(diagnostic.rule(), Some("TestRule"));
    assert_eq!(diagnostic.message(), "test warning");
}
