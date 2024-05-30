//! Macros used in testing.

/// Scaffolds a test to ensure that a targeted entity is able to be constructed
/// from a valid parsed node using `try_from()`.
///
/// # Arguments
///
/// * `$input` - a [`&str`](str) that is parsed into the defined `$type_`.
/// * `$type_` - the name of the rule to parse the `$input` as.
/// * `$target` - the name of the entity to attempt to construct from the
///   `$type_`.
pub macro valid_node($input:literal, $type_:ident, $target:ident) {{
    let node = wdl_grammar::v1::parse_rule(wdl_grammar::v1::Rule::$type_, $input, false)
        .unwrap()
        .into_tree()
        .unwrap();

    $target::try_from(node).unwrap()
}}

/// Scaffolds a test to ensure that a targeted entity fails to be constructed
/// from an invalid parsed node using `try_from()`.
///
/// # Arguments
///
/// * `$input` - a [`&str`](str) that is parsed into the defined `$type_`.
/// * `$type_` - the name of the rule to parse the `$input` as.
/// * `$name` - the name of the target entity included in the error message.
/// * `$target` - the name of the target entity to attempt to construct from the
///   `$type_` as a Rust identifier.
pub macro create_invalid_node_test($input:literal, $type_:ident, $name:ident, $target:ident, $test_name:ident) {
    #[test]
    fn $test_name() {
        let expected_panic_message = format!(
            "{} cannot be parsed from node type {:?}",
            stringify!($name),
            wdl_grammar::v1::Rule::$type_
        );

        let result = std::panic::catch_unwind(|| {
            let parse_node =
                wdl_grammar::v1::parse_rule(wdl_grammar::v1::Rule::$type_, $input, false)
                    .unwrap()
                    .into_tree()
                    .unwrap();

            $target::try_from(parse_node)
        });

        let error = result.unwrap_err();
        if let Some(panic_message) = error.downcast_ref::<String>() {
            assert_eq!(
                panic_message, &expected_panic_message,
                "Panic message does not match the expected message"
            );
        } else if let Some(panic_message) = error.downcast_ref::<&'static str>() {
            assert_eq!(
                panic_message, &expected_panic_message,
                "Panic message does not match the expected message"
            );
        } else {
            panic!("Test panicked with a non-string message");
        }
    }
}
