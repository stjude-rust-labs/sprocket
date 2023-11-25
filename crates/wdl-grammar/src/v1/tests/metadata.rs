use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

mod parameter_metadata;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_literal() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::metadata,
        positives: vec![Rule::metadata],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_an_empty_metadata_object() {
    parses_to! {
        parser: WdlParser,
        input: "meta {}",
        rule: Rule::metadata,
        tokens: [
            // `meta {}`
            metadata(0, 7, [
                WHITESPACE(4, 5, [
                    SPACE(4, 5),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_empty_metadata_object_with_spaces() {
    parses_to! {
        parser: WdlParser,
        input: "meta   {   }",
        rule: Rule::metadata,
        tokens: [
            // `meta   {   }`
            metadata(0, 12, [
                WHITESPACE(4, 5, [
                    SPACE(4, 5),
                ]),
                WHITESPACE(5, 6, [
                    SPACE(5, 6),
                ]),
                WHITESPACE(6, 7, [
                    SPACE(6, 7),
                ]),
                WHITESPACE(8, 9, [
                    SPACE(8, 9),
                ]),
                WHITESPACE(9, 10, [
                    SPACE(9, 10),
                ]),
                WHITESPACE(10, 11, [
                    SPACE(10, 11),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_metadata_object_with_keys() {
    parses_to! {
        parser: WdlParser,
        input: r#"meta { hello: "world" foo: null }"#,
        rule: Rule::metadata,
        tokens: [
            // `meta { hello: "world" foo: null }`
            metadata(0, 33, [
                WHITESPACE(4, 5, [
                    SPACE(4, 5),
                ]),
                WHITESPACE(6, 7, [
                    SPACE(6, 7),
                ]),
                // `hello: "world"`
                metadata_kv(7, 21, [
                    // `hello`
                    metadata_key(7, 12, [
                        // `hello`
                        singular_identifier(7, 12),
                    ]),
                    WHITESPACE(13, 14, [
                        SPACE(13, 14),
                    ]),
                    // `"world"`
                    metadata_value(14, 21, [
                        // `"world"`
                        string(14, 21, [
                            // `"`
                            double_quote(14, 15),
                            // `world`
                            string_inner(15, 20, [
                                // `world`
                                string_literal_contents(15, 20),
                            ]),
                        ]),
                    ]),
                ]),
                WHITESPACE(21, 22, [
                    SPACE(21, 22),
                ]),
                // `foo: null`
                metadata_kv(22, 31, [
                    // `foo`
                    metadata_key(22, 25, [
                        // `foo`
                        singular_identifier(22, 25),
                    ]),
                    WHITESPACE(26, 27, [
                        SPACE(26, 27),
                    ]),
                    // `null`
                    metadata_value(27, 31, [
                        // `null`
                        null(27, 31),
                    ]),
                ]),
                WHITESPACE(31, 32, [
                    SPACE(31, 32),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_multiline_metadata_object_with_keys() {
    parses_to! {
        parser: WdlParser,
        input: r#"meta {
    hello: "world"
    foo: null
}"#,
        rule: Rule::metadata,
        tokens: [
            // `meta { hello: "world" foo: null }`
            metadata(0, 41, [
                WHITESPACE(4, 5, [
                    SPACE(4, 5),
                ]),
                WHITESPACE(6, 7, [
                    NEWLINE(6, 7),
                ]),
                WHITESPACE(7, 8, [
                    SPACE(7, 8),
                ]),
                WHITESPACE(8, 9, [
                    SPACE(8, 9),
                ]),
                WHITESPACE(9, 10, [
                    SPACE(9, 10),
                ]),
                WHITESPACE(10, 11, [
                    SPACE(10, 11),
                ]),
                // `hello: "world"`
                metadata_kv(11, 25, [
                // `hello`
                metadata_key(11, 16, [
                    // `hello`
                    singular_identifier(11, 16),
                ]),
                WHITESPACE(17, 18, [
                    SPACE(17, 18),
                ]),
                // `"world"`
                metadata_value(18, 25, [
                    // `"world"`
                    string(18, 25, [
                        // `"`
                        double_quote(18, 19),
                            // `world`
                            string_inner(19, 24, [
                                // `world`
                                string_literal_contents(19, 24),
                            ]),
                        ]),
                    ]),
                ]),
                WHITESPACE(25, 26, [
                    NEWLINE(25, 26),
                ]),
                WHITESPACE(26, 27, [
                    SPACE(26, 27),
                ]),
                WHITESPACE(27, 28, [
                    SPACE(27, 28),
                ]),
                WHITESPACE(28, 29, [
                    SPACE(28, 29),
                ]),
                WHITESPACE(29, 30, [
                    SPACE(29, 30),
                ]),
                // `foo: null`
                metadata_kv(30, 39, [
                    // `foo`
                    metadata_key(30, 33, [
                        // `foo`
                        singular_identifier(30, 33),
                    ]),
                    WHITESPACE(34, 35, [
                        SPACE(34, 35),
                    ]),
                    // `null`
                    metadata_value(35, 39, [
                        // `null`
                        null(35, 39),
                    ]),
                ]),
                WHITESPACE(39, 40, [
                    NEWLINE(39, 40),
                ]),
            ])
        ]
    }
}

// ============== //
// Testing Commas //
// ============== //

// All of the tests below are intended to test the correct usage of commas,
// which is a bit of a tricky subject. Generally speaking, commas are not
// allowed after top-level keys in `meta` and `parameter_meta` sections. That
// said, they are _required_ to delimit items within metadata object values and
// metadata array values (unless there is a single item in either).

#[test]
fn it_fails_to_parse_a_metadata_with_top_level_commas() {
    fails_with! {
        parser: WdlParser,
        input: r#"meta {
    hello: "world",
    foo: null
}"#,
        rule: Rule::metadata,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT],
        negatives: vec![Rule::COMMA],
        pos: 25
    }
}

#[test]
fn it_fails_to_parse_a_metadata_without_commas_within_a_metadata_object() {
    fails_with! {
        parser: WdlParser,
        input: r#"meta {
    hello: {
        foo: "bar"
        baz: "quux"
    }
}"#,
        rule: Rule::metadata,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT, Rule::COMMA],
        negatives: vec![],
        pos: 47
    }
}

#[test]
fn it_fails_to_parse_a_metadata_without_commas_within_a_metadata_array() {
    fails_with! {
        parser: WdlParser,
        input: r#"meta {
    hello: {
        foo: [
            "bar"
            "quux"
        ]
    }
}"#,
        rule: Rule::metadata,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT, Rule::COMMA],
        negatives: vec![],
        pos: 65
    }
}

#[test]
fn it_successfully_parse_a_metadata_with_commas_within_a_metadata_object() {
    parses_to! {
        parser: WdlParser,
        input: r#"meta {
    hello: {
        foo: "bar",
        baz: "quux"
    }
}"#,
        rule: Rule::metadata,
        tokens: [
            // `meta { hello: { foo: "bar", baz: "quux" } }`
            metadata(0, 67, [
                WHITESPACE(4, 5, [
                  SPACE(4, 5),
                ]),
                WHITESPACE(6, 7, [
                  NEWLINE(6, 7),
                ]),
                WHITESPACE(7, 8, [
                  SPACE(7, 8),
                ]),
                WHITESPACE(8, 9, [
                  SPACE(8, 9),
                ]),
                WHITESPACE(9, 10, [
                  SPACE(9, 10),
                ]),
                WHITESPACE(10, 11, [
                  SPACE(10, 11),
                ]),
                // `hello: { foo: "bar", baz: "quux" }`
                metadata_kv(11, 65, [
                  // `hello`
                  metadata_key(11, 16, [
                    // `hello`
                    singular_identifier(11, 16),
                  ]),
                  WHITESPACE(17, 18, [
                    SPACE(17, 18),
                  ]),
                  // `{ foo: "bar", baz: "quux" }`
                  metadata_value(18, 65, [
                    // `{ foo: "bar", baz: "quux" }`
                    metadata_object(18, 65, [
                      WHITESPACE(19, 20, [
                        NEWLINE(19, 20),
                      ]),
                      WHITESPACE(20, 21, [
                        SPACE(20, 21),
                      ]),
                      WHITESPACE(21, 22, [
                        SPACE(21, 22),
                      ]),
                      WHITESPACE(22, 23, [
                        SPACE(22, 23),
                      ]),
                      WHITESPACE(23, 24, [
                        SPACE(23, 24),
                      ]),
                      WHITESPACE(24, 25, [
                        SPACE(24, 25),
                      ]),
                      WHITESPACE(25, 26, [
                        SPACE(25, 26),
                      ]),
                      WHITESPACE(26, 27, [
                        SPACE(26, 27),
                      ]),
                      WHITESPACE(27, 28, [
                        SPACE(27, 28),
                      ]),
                      // `foo: "bar"`
                      metadata_kv(28, 38, [
                        // `foo`
                        metadata_key(28, 31, [
                          // `foo`
                          singular_identifier(28, 31),
                        ]),
                        WHITESPACE(32, 33, [
                          SPACE(32, 33),
                        ]),
                        // `"bar"`
                        metadata_value(33, 38, [
                          // `"bar"`
                          string(33, 38, [
                            // `"`
                            double_quote(33, 34),
                            // `bar`
                            string_inner(34, 37, [
                              // `bar`
                              string_literal_contents(34, 37),
                            ]),
                          ]),
                        ]),
                      ]),
                      // `,`
                      COMMA(38, 39),
                      WHITESPACE(39, 40, [
                        NEWLINE(39, 40),
                      ]),
                      WHITESPACE(40, 41, [
                        SPACE(40, 41),
                      ]),
                      WHITESPACE(41, 42, [
                        SPACE(41, 42),
                      ]),
                      WHITESPACE(42, 43, [
                        SPACE(42, 43),
                      ]),
                      WHITESPACE(43, 44, [
                        SPACE(43, 44),
                      ]),
                      WHITESPACE(44, 45, [
                        SPACE(44, 45),
                      ]),
                      WHITESPACE(45, 46, [
                        SPACE(45, 46),
                      ]),
                      WHITESPACE(46, 47, [
                        SPACE(46, 47),
                      ]),
                      WHITESPACE(47, 48, [
                        SPACE(47, 48),
                      ]),
                      // `baz: "quux"`
                      metadata_kv(48, 59, [
                        // `baz`
                        metadata_key(48, 51, [
                          // `baz`
                          singular_identifier(48, 51),
                        ]),
                        WHITESPACE(52, 53, [
                          SPACE(52, 53),
                        ]),
                        // `"quux"`
                        metadata_value(53, 59, [
                          // `"quux"`
                          string(53, 59, [
                            // `"`
                            double_quote(53, 54),
                            // `quux`
                            string_inner(54, 58, [
                              // `quux`
                              string_literal_contents(54, 58),
                            ]),
                          ]),
                        ]),
                      ]),
                      WHITESPACE(59, 60, [
                        NEWLINE(59, 60),
                      ]),
                      WHITESPACE(60, 61, [
                        SPACE(60, 61),
                      ]),
                      WHITESPACE(61, 62, [
                        SPACE(61, 62),
                      ]),
                      WHITESPACE(62, 63, [
                        SPACE(62, 63),
                      ]),
                      WHITESPACE(63, 64, [
                        SPACE(63, 64),
                      ]),
                    ]),
                  ]),
                ]),
                WHITESPACE(65, 66, [
                  NEWLINE(65, 66),
                ]),
              ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_metadata_with_commas_within_a_metadata_array() {
    parses_to! {
        parser: WdlParser,
        input: r#"meta {
    hello: {
        foo: [
            "bar",
            "quux"
        ]
    }
}"#,
        rule: Rule::metadata,
        tokens: [
            // `meta { hello: { foo: [ "bar", "quux" ] } }`
            metadata(0, 90, [
                WHITESPACE(4, 5, [
                SPACE(4, 5),
                ]),
                WHITESPACE(6, 7, [
                NEWLINE(6, 7),
                ]),
                WHITESPACE(7, 8, [
                SPACE(7, 8),
                ]),
                WHITESPACE(8, 9, [
                SPACE(8, 9),
                ]),
                WHITESPACE(9, 10, [
                SPACE(9, 10),
                ]),
                WHITESPACE(10, 11, [
                SPACE(10, 11),
                ]),
                // `hello: { foo: [ "bar", "quux" ] }`
                metadata_kv(11, 88, [
                // `hello`
                metadata_key(11, 16, [
                    // `hello`
                    singular_identifier(11, 16),
                ]),
                WHITESPACE(17, 18, [
                    SPACE(17, 18),
                ]),
                // `{ foo: [ "bar", "quux" ] }`
                metadata_value(18, 88, [
                    // `{ foo: [ "bar", "quux" ] }`
                    metadata_object(18, 88, [
                    WHITESPACE(19, 20, [
                        NEWLINE(19, 20),
                    ]),
                    WHITESPACE(20, 21, [
                        SPACE(20, 21),
                    ]),
                    WHITESPACE(21, 22, [
                        SPACE(21, 22),
                    ]),
                    WHITESPACE(22, 23, [
                        SPACE(22, 23),
                    ]),
                    WHITESPACE(23, 24, [
                        SPACE(23, 24),
                    ]),
                    WHITESPACE(24, 25, [
                        SPACE(24, 25),
                    ]),
                    WHITESPACE(25, 26, [
                        SPACE(25, 26),
                    ]),
                    WHITESPACE(26, 27, [
                        SPACE(26, 27),
                    ]),
                    WHITESPACE(27, 28, [
                        SPACE(27, 28),
                    ]),
                    // `foo: [ "bar", "quux" ]`
                    metadata_kv(28, 82, [
                        // `foo`
                        metadata_key(28, 31, [
                        // `foo`
                        singular_identifier(28, 31),
                        ]),
                        WHITESPACE(32, 33, [
                        SPACE(32, 33),
                        ]),
                        // `[ "bar", "quux" ]`
                        metadata_value(33, 82, [
                        // `[ "bar", "quux" ]`
                        metadata_array(33, 82, [
                            WHITESPACE(34, 35, [
                            NEWLINE(34, 35),
                            ]),
                            WHITESPACE(35, 36, [
                            SPACE(35, 36),
                            ]),
                            WHITESPACE(36, 37, [
                            SPACE(36, 37),
                            ]),
                            WHITESPACE(37, 38, [
                            SPACE(37, 38),
                            ]),
                            WHITESPACE(38, 39, [
                            SPACE(38, 39),
                            ]),
                            WHITESPACE(39, 40, [
                            SPACE(39, 40),
                            ]),
                            WHITESPACE(40, 41, [
                            SPACE(40, 41),
                            ]),
                            WHITESPACE(41, 42, [
                            SPACE(41, 42),
                            ]),
                            WHITESPACE(42, 43, [
                            SPACE(42, 43),
                            ]),
                            WHITESPACE(43, 44, [
                            SPACE(43, 44),
                            ]),
                            WHITESPACE(44, 45, [
                            SPACE(44, 45),
                            ]),
                            WHITESPACE(45, 46, [
                            SPACE(45, 46),
                            ]),
                            WHITESPACE(46, 47, [
                            SPACE(46, 47),
                            ]),
                            // `"bar"`
                            metadata_value(47, 52, [
                            // `"bar"`
                            string(47, 52, [
                                // `"`
                                double_quote(47, 48),
                                // `bar`
                                string_inner(48, 51, [
                                // `bar`
                                string_literal_contents(48, 51),
                                ]),
                            ]),
                            ]),
                            // `,`
                            COMMA(52, 53),
                            WHITESPACE(53, 54, [
                            NEWLINE(53, 54),
                            ]),
                            WHITESPACE(54, 55, [
                            SPACE(54, 55),
                            ]),
                            WHITESPACE(55, 56, [
                            SPACE(55, 56),
                            ]),
                            WHITESPACE(56, 57, [
                            SPACE(56, 57),
                            ]),
                            WHITESPACE(57, 58, [
                            SPACE(57, 58),
                            ]),
                            WHITESPACE(58, 59, [
                            SPACE(58, 59),
                            ]),
                            WHITESPACE(59, 60, [
                            SPACE(59, 60),
                            ]),
                            WHITESPACE(60, 61, [
                            SPACE(60, 61),
                            ]),
                            WHITESPACE(61, 62, [
                            SPACE(61, 62),
                            ]),
                            WHITESPACE(62, 63, [
                            SPACE(62, 63),
                            ]),
                            WHITESPACE(63, 64, [
                            SPACE(63, 64),
                            ]),
                            WHITESPACE(64, 65, [
                            SPACE(64, 65),
                            ]),
                            WHITESPACE(65, 66, [
                            SPACE(65, 66),
                            ]),
                            // `"quux"`
                            metadata_value(66, 72, [
                            // `"quux"`
                            string(66, 72, [
                                // `"`
                                double_quote(66, 67),
                                // `quux`
                                string_inner(67, 71, [
                                // `quux`
                                string_literal_contents(67, 71),
                                ]),
                            ]),
                            ]),
                            WHITESPACE(72, 73, [
                            NEWLINE(72, 73),
                            ]),
                            WHITESPACE(73, 74, [
                            SPACE(73, 74),
                            ]),
                            WHITESPACE(74, 75, [
                            SPACE(74, 75),
                            ]),
                            WHITESPACE(75, 76, [
                            SPACE(75, 76),
                            ]),
                            WHITESPACE(76, 77, [
                            SPACE(76, 77),
                            ]),
                            WHITESPACE(77, 78, [
                            SPACE(77, 78),
                            ]),
                            WHITESPACE(78, 79, [
                            SPACE(78, 79),
                            ]),
                            WHITESPACE(79, 80, [
                            SPACE(79, 80),
                            ]),
                            WHITESPACE(80, 81, [
                            SPACE(80, 81),
                            ]),
                        ]),
                        ]),
                    ]),
                    WHITESPACE(82, 83, [
                        NEWLINE(82, 83),
                    ]),
                    WHITESPACE(83, 84, [
                        SPACE(83, 84),
                    ]),
                    WHITESPACE(84, 85, [
                        SPACE(84, 85),
                    ]),
                    WHITESPACE(85, 86, [
                        SPACE(85, 86),
                    ]),
                    WHITESPACE(86, 87, [
                        SPACE(86, 87),
                    ]),
                    ]),
                ]),
                ]),
                WHITESPACE(88, 89, [
                NEWLINE(88, 89),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parse_a_metadata_with_commas_within_a_metadata_object_with_a_trailing_comma() {
    parses_to! {
        parser: WdlParser,
        input: r#"meta {
    hello: {
        foo: "bar",
        baz: "quux",
    }
}"#,
        rule: Rule::metadata,
        tokens: [
            // `meta { hello: { foo: "bar", baz: "quux", } }`
            metadata(0, 68, [
                WHITESPACE(4, 5, [
                SPACE(4, 5),
                ]),
                WHITESPACE(6, 7, [
                NEWLINE(6, 7),
                ]),
                WHITESPACE(7, 8, [
                SPACE(7, 8),
                ]),
                WHITESPACE(8, 9, [
                SPACE(8, 9),
                ]),
                WHITESPACE(9, 10, [
                SPACE(9, 10),
                ]),
                WHITESPACE(10, 11, [
                SPACE(10, 11),
                ]),
                // `hello: { foo: "bar", baz: "quux", }`
                metadata_kv(11, 66, [
                // `hello`
                metadata_key(11, 16, [
                    // `hello`
                    singular_identifier(11, 16),
                ]),
                WHITESPACE(17, 18, [
                    SPACE(17, 18),
                ]),
                // `{ foo: "bar", baz: "quux", }`
                metadata_value(18, 66, [
                    // `{ foo: "bar", baz: "quux", }`
                    metadata_object(18, 66, [
                    WHITESPACE(19, 20, [
                        NEWLINE(19, 20),
                    ]),
                    WHITESPACE(20, 21, [
                        SPACE(20, 21),
                    ]),
                    WHITESPACE(21, 22, [
                        SPACE(21, 22),
                    ]),
                    WHITESPACE(22, 23, [
                        SPACE(22, 23),
                    ]),
                    WHITESPACE(23, 24, [
                        SPACE(23, 24),
                    ]),
                    WHITESPACE(24, 25, [
                        SPACE(24, 25),
                    ]),
                    WHITESPACE(25, 26, [
                        SPACE(25, 26),
                    ]),
                    WHITESPACE(26, 27, [
                        SPACE(26, 27),
                    ]),
                    WHITESPACE(27, 28, [
                        SPACE(27, 28),
                    ]),
                    // `foo: "bar"`
                    metadata_kv(28, 38, [
                        // `foo`
                        metadata_key(28, 31, [
                        // `foo`
                        singular_identifier(28, 31),
                        ]),
                        WHITESPACE(32, 33, [
                        SPACE(32, 33),
                        ]),
                        // `"bar"`
                        metadata_value(33, 38, [
                        // `"bar"`
                        string(33, 38, [
                            // `"`
                            double_quote(33, 34),
                            // `bar`
                            string_inner(34, 37, [
                            // `bar`
                            string_literal_contents(34, 37),
                            ]),
                        ]),
                        ]),
                    ]),
                    // `,`
                    COMMA(38, 39),
                    WHITESPACE(39, 40, [
                        NEWLINE(39, 40),
                    ]),
                    WHITESPACE(40, 41, [
                        SPACE(40, 41),
                    ]),
                    WHITESPACE(41, 42, [
                        SPACE(41, 42),
                    ]),
                    WHITESPACE(42, 43, [
                        SPACE(42, 43),
                    ]),
                    WHITESPACE(43, 44, [
                        SPACE(43, 44),
                    ]),
                    WHITESPACE(44, 45, [
                        SPACE(44, 45),
                    ]),
                    WHITESPACE(45, 46, [
                        SPACE(45, 46),
                    ]),
                    WHITESPACE(46, 47, [
                        SPACE(46, 47),
                    ]),
                    WHITESPACE(47, 48, [
                        SPACE(47, 48),
                    ]),
                    // `baz: "quux"`
                    metadata_kv(48, 59, [
                        // `baz`
                        metadata_key(48, 51, [
                        // `baz`
                        singular_identifier(48, 51),
                        ]),
                        WHITESPACE(52, 53, [
                        SPACE(52, 53),
                        ]),
                        // `"quux"`
                        metadata_value(53, 59, [
                        // `"quux"`
                        string(53, 59, [
                            // `"`
                            double_quote(53, 54),
                            // `quux`
                            string_inner(54, 58, [
                            // `quux`
                            string_literal_contents(54, 58),
                            ]),
                        ]),
                        ]),
                    ]),
                    // `,`
                    COMMA(59, 60),
                    WHITESPACE(60, 61, [
                        NEWLINE(60, 61),
                    ]),
                    WHITESPACE(61, 62, [
                        SPACE(61, 62),
                    ]),
                    WHITESPACE(62, 63, [
                        SPACE(62, 63),
                    ]),
                    WHITESPACE(63, 64, [
                        SPACE(63, 64),
                    ]),
                    WHITESPACE(64, 65, [
                        SPACE(64, 65),
                    ]),
                    ]),
                ]),
                ]),
                WHITESPACE(66, 67, [
                NEWLINE(66, 67),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_metadata_with_commas_within_a_metadata_array_with_a_trailing_comma() {
    parses_to! {
        parser: WdlParser,
        input: r#"meta {
    hello: {
        foo: [
            "bar",
            "quux",
        ]
    }
}"#,
        rule: Rule::metadata,
        tokens: [
            // `meta { hello: { foo: [ "bar", "quux", ] } }`
            metadata(0, 91, [
                WHITESPACE(4, 5, [
                SPACE(4, 5),
                ]),
                WHITESPACE(6, 7, [
                NEWLINE(6, 7),
                ]),
                WHITESPACE(7, 8, [
                SPACE(7, 8),
                ]),
                WHITESPACE(8, 9, [
                SPACE(8, 9),
                ]),
                WHITESPACE(9, 10, [
                SPACE(9, 10),
                ]),
                WHITESPACE(10, 11, [
                SPACE(10, 11),
                ]),
                // `hello: { foo: [ "bar", "quux", ] }`
                metadata_kv(11, 89, [
                // `hello`
                metadata_key(11, 16, [
                    // `hello`
                    singular_identifier(11, 16),
                ]),
                WHITESPACE(17, 18, [
                    SPACE(17, 18),
                ]),
                // `{ foo: [ "bar", "quux", ] }`
                metadata_value(18, 89, [
                    // `{ foo: [ "bar", "quux", ] }`
                    metadata_object(18, 89, [
                    WHITESPACE(19, 20, [
                        NEWLINE(19, 20),
                    ]),
                    WHITESPACE(20, 21, [
                        SPACE(20, 21),
                    ]),
                    WHITESPACE(21, 22, [
                        SPACE(21, 22),
                    ]),
                    WHITESPACE(22, 23, [
                        SPACE(22, 23),
                    ]),
                    WHITESPACE(23, 24, [
                        SPACE(23, 24),
                    ]),
                    WHITESPACE(24, 25, [
                        SPACE(24, 25),
                    ]),
                    WHITESPACE(25, 26, [
                        SPACE(25, 26),
                    ]),
                    WHITESPACE(26, 27, [
                        SPACE(26, 27),
                    ]),
                    WHITESPACE(27, 28, [
                        SPACE(27, 28),
                    ]),
                    // `foo: [ "bar", "quux", ]`
                    metadata_kv(28, 83, [
                        // `foo`
                        metadata_key(28, 31, [
                        // `foo`
                        singular_identifier(28, 31),
                        ]),
                        WHITESPACE(32, 33, [
                        SPACE(32, 33),
                        ]),
                        // `[ "bar", "quux", ]`
                        metadata_value(33, 83, [
                        // `[ "bar", "quux", ]`
                        metadata_array(33, 83, [
                            WHITESPACE(34, 35, [
                            NEWLINE(34, 35),
                            ]),
                            WHITESPACE(35, 36, [
                            SPACE(35, 36),
                            ]),
                            WHITESPACE(36, 37, [
                            SPACE(36, 37),
                            ]),
                            WHITESPACE(37, 38, [
                            SPACE(37, 38),
                            ]),
                            WHITESPACE(38, 39, [
                            SPACE(38, 39),
                            ]),
                            WHITESPACE(39, 40, [
                            SPACE(39, 40),
                            ]),
                            WHITESPACE(40, 41, [
                            SPACE(40, 41),
                            ]),
                            WHITESPACE(41, 42, [
                            SPACE(41, 42),
                            ]),
                            WHITESPACE(42, 43, [
                            SPACE(42, 43),
                            ]),
                            WHITESPACE(43, 44, [
                            SPACE(43, 44),
                            ]),
                            WHITESPACE(44, 45, [
                            SPACE(44, 45),
                            ]),
                            WHITESPACE(45, 46, [
                            SPACE(45, 46),
                            ]),
                            WHITESPACE(46, 47, [
                            SPACE(46, 47),
                            ]),
                            // `"bar"`
                            metadata_value(47, 52, [
                            // `"bar"`
                            string(47, 52, [
                                // `"`
                                double_quote(47, 48),
                                // `bar`
                                string_inner(48, 51, [
                                // `bar`
                                string_literal_contents(48, 51),
                                ]),
                            ]),
                            ]),
                            // `,`
                            COMMA(52, 53),
                            WHITESPACE(53, 54, [
                            NEWLINE(53, 54),
                            ]),
                            WHITESPACE(54, 55, [
                            SPACE(54, 55),
                            ]),
                            WHITESPACE(55, 56, [
                            SPACE(55, 56),
                            ]),
                            WHITESPACE(56, 57, [
                            SPACE(56, 57),
                            ]),
                            WHITESPACE(57, 58, [
                            SPACE(57, 58),
                            ]),
                            WHITESPACE(58, 59, [
                            SPACE(58, 59),
                            ]),
                            WHITESPACE(59, 60, [
                            SPACE(59, 60),
                            ]),
                            WHITESPACE(60, 61, [
                            SPACE(60, 61),
                            ]),
                            WHITESPACE(61, 62, [
                            SPACE(61, 62),
                            ]),
                            WHITESPACE(62, 63, [
                            SPACE(62, 63),
                            ]),
                            WHITESPACE(63, 64, [
                            SPACE(63, 64),
                            ]),
                            WHITESPACE(64, 65, [
                            SPACE(64, 65),
                            ]),
                            WHITESPACE(65, 66, [
                            SPACE(65, 66),
                            ]),
                            // `"quux"`
                            metadata_value(66, 72, [
                            // `"quux"`
                            string(66, 72, [
                                // `"`
                                double_quote(66, 67),
                                // `quux`
                                string_inner(67, 71, [
                                // `quux`
                                string_literal_contents(67, 71),
                                ]),
                            ]),
                            ]),
                            // `,`
                            COMMA(72, 73),
                            WHITESPACE(73, 74, [
                            NEWLINE(73, 74),
                            ]),
                            WHITESPACE(74, 75, [
                            SPACE(74, 75),
                            ]),
                            WHITESPACE(75, 76, [
                            SPACE(75, 76),
                            ]),
                            WHITESPACE(76, 77, [
                            SPACE(76, 77),
                            ]),
                            WHITESPACE(77, 78, [
                            SPACE(77, 78),
                            ]),
                            WHITESPACE(78, 79, [
                            SPACE(78, 79),
                            ]),
                            WHITESPACE(79, 80, [
                            SPACE(79, 80),
                            ]),
                            WHITESPACE(80, 81, [
                            SPACE(80, 81),
                            ]),
                            WHITESPACE(81, 82, [
                            SPACE(81, 82),
                            ]),
                        ]),
                        ]),
                    ]),
                    WHITESPACE(83, 84, [
                        NEWLINE(83, 84),
                    ]),
                    WHITESPACE(84, 85, [
                        SPACE(84, 85),
                    ]),
                    WHITESPACE(85, 86, [
                        SPACE(85, 86),
                    ]),
                    WHITESPACE(86, 87, [
                        SPACE(86, 87),
                    ]),
                    WHITESPACE(87, 88, [
                        SPACE(87, 88),
                    ]),
                    ]),
                ]),
                ]),
                WHITESPACE(89, 90, [
                NEWLINE(89, 90),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parse_a_metadata_object_with_a_single_element_and_no_commas() {
    parses_to! {
        parser: WdlParser,
        input: r#"meta {
    hello: {
        foo: "bar"
    }
}"#,
        rule: Rule::metadata,
        tokens: [
            // `meta { hello: { foo: "bar" } }`
            metadata(0, 46, [
                WHITESPACE(4, 5, [
                SPACE(4, 5),
                ]),
                WHITESPACE(6, 7, [
                NEWLINE(6, 7),
                ]),
                WHITESPACE(7, 8, [
                SPACE(7, 8),
                ]),
                WHITESPACE(8, 9, [
                SPACE(8, 9),
                ]),
                WHITESPACE(9, 10, [
                SPACE(9, 10),
                ]),
                WHITESPACE(10, 11, [
                SPACE(10, 11),
                ]),
                // `hello: { foo: "bar" }`
                metadata_kv(11, 44, [
                // `hello`
                metadata_key(11, 16, [
                    // `hello`
                    singular_identifier(11, 16),
                ]),
                WHITESPACE(17, 18, [
                    SPACE(17, 18),
                ]),
                // `{ foo: "bar" }`
                metadata_value(18, 44, [
                    // `{ foo: "bar" }`
                    metadata_object(18, 44, [
                    WHITESPACE(19, 20, [
                        NEWLINE(19, 20),
                    ]),
                    WHITESPACE(20, 21, [
                        SPACE(20, 21),
                    ]),
                    WHITESPACE(21, 22, [
                        SPACE(21, 22),
                    ]),
                    WHITESPACE(22, 23, [
                        SPACE(22, 23),
                    ]),
                    WHITESPACE(23, 24, [
                        SPACE(23, 24),
                    ]),
                    WHITESPACE(24, 25, [
                        SPACE(24, 25),
                    ]),
                    WHITESPACE(25, 26, [
                        SPACE(25, 26),
                    ]),
                    WHITESPACE(26, 27, [
                        SPACE(26, 27),
                    ]),
                    WHITESPACE(27, 28, [
                        SPACE(27, 28),
                    ]),
                    // `foo: "bar"`
                    metadata_kv(28, 38, [
                        // `foo`
                        metadata_key(28, 31, [
                        // `foo`
                        singular_identifier(28, 31),
                        ]),
                        WHITESPACE(32, 33, [
                        SPACE(32, 33),
                        ]),
                        // `"bar"`
                        metadata_value(33, 38, [
                        // `"bar"`
                        string(33, 38, [
                            // `"`
                            double_quote(33, 34),
                            // `bar`
                            string_inner(34, 37, [
                            // `bar`
                            string_literal_contents(34, 37),
                            ]),
                        ]),
                        ]),
                    ]),
                    WHITESPACE(38, 39, [
                        NEWLINE(38, 39),
                    ]),
                    WHITESPACE(39, 40, [
                        SPACE(39, 40),
                    ]),
                    WHITESPACE(40, 41, [
                        SPACE(40, 41),
                    ]),
                    WHITESPACE(41, 42, [
                        SPACE(41, 42),
                    ]),
                    WHITESPACE(42, 43, [
                        SPACE(42, 43),
                    ]),
                    ]),
                ]),
                ]),
                WHITESPACE(44, 45, [
                NEWLINE(44, 45),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_metadata_array_with_a_single_element_and_no_commas() {
    parses_to! {
        parser: WdlParser,
        input: r#"meta {
    hello: {
        foo: [
            "bar"
        ]
    }
}"#,
        rule: Rule::metadata,
        tokens: [
            // `meta { hello: { foo: [ "bar" ] } }`
            metadata(0, 70, [
                WHITESPACE(4, 5, [
                SPACE(4, 5),
                ]),
                WHITESPACE(6, 7, [
                NEWLINE(6, 7),
                ]),
                WHITESPACE(7, 8, [
                SPACE(7, 8),
                ]),
                WHITESPACE(8, 9, [
                SPACE(8, 9),
                ]),
                WHITESPACE(9, 10, [
                SPACE(9, 10),
                ]),
                WHITESPACE(10, 11, [
                SPACE(10, 11),
                ]),
                // `hello: { foo: [ "bar" ] }`
                metadata_kv(11, 68, [
                // `hello`
                metadata_key(11, 16, [
                    // `hello`
                    singular_identifier(11, 16),
                ]),
                WHITESPACE(17, 18, [
                    SPACE(17, 18),
                ]),
                // `{ foo: [ "bar" ] }`
                metadata_value(18, 68, [
                    // `{ foo: [ "bar" ] }`
                    metadata_object(18, 68, [
                    WHITESPACE(19, 20, [
                        NEWLINE(19, 20),
                    ]),
                    WHITESPACE(20, 21, [
                        SPACE(20, 21),
                    ]),
                    WHITESPACE(21, 22, [
                        SPACE(21, 22),
                    ]),
                    WHITESPACE(22, 23, [
                        SPACE(22, 23),
                    ]),
                    WHITESPACE(23, 24, [
                        SPACE(23, 24),
                    ]),
                    WHITESPACE(24, 25, [
                        SPACE(24, 25),
                    ]),
                    WHITESPACE(25, 26, [
                        SPACE(25, 26),
                    ]),
                    WHITESPACE(26, 27, [
                        SPACE(26, 27),
                    ]),
                    WHITESPACE(27, 28, [
                        SPACE(27, 28),
                    ]),
                    // `foo: [ "bar" ]`
                    metadata_kv(28, 62, [
                        // `foo`
                        metadata_key(28, 31, [
                        // `foo`
                        singular_identifier(28, 31),
                        ]),
                        WHITESPACE(32, 33, [
                        SPACE(32, 33),
                        ]),
                        // `[ "bar" ]`
                        metadata_value(33, 62, [
                        // `[ "bar" ]`
                        metadata_array(33, 62, [
                            WHITESPACE(34, 35, [
                            NEWLINE(34, 35),
                            ]),
                            WHITESPACE(35, 36, [
                            SPACE(35, 36),
                            ]),
                            WHITESPACE(36, 37, [
                            SPACE(36, 37),
                            ]),
                            WHITESPACE(37, 38, [
                            SPACE(37, 38),
                            ]),
                            WHITESPACE(38, 39, [
                            SPACE(38, 39),
                            ]),
                            WHITESPACE(39, 40, [
                            SPACE(39, 40),
                            ]),
                            WHITESPACE(40, 41, [
                            SPACE(40, 41),
                            ]),
                            WHITESPACE(41, 42, [
                            SPACE(41, 42),
                            ]),
                            WHITESPACE(42, 43, [
                            SPACE(42, 43),
                            ]),
                            WHITESPACE(43, 44, [
                            SPACE(43, 44),
                            ]),
                            WHITESPACE(44, 45, [
                            SPACE(44, 45),
                            ]),
                            WHITESPACE(45, 46, [
                            SPACE(45, 46),
                            ]),
                            WHITESPACE(46, 47, [
                            SPACE(46, 47),
                            ]),
                            // `"bar"`
                            metadata_value(47, 52, [
                            // `"bar"`
                            string(47, 52, [
                                // `"`
                                double_quote(47, 48),
                                // `bar`
                                string_inner(48, 51, [
                                // `bar`
                                string_literal_contents(48, 51),
                                ]),
                            ]),
                            ]),
                            WHITESPACE(52, 53, [
                            NEWLINE(52, 53),
                            ]),
                            WHITESPACE(53, 54, [
                            SPACE(53, 54),
                            ]),
                            WHITESPACE(54, 55, [
                            SPACE(54, 55),
                            ]),
                            WHITESPACE(55, 56, [
                            SPACE(55, 56),
                            ]),
                            WHITESPACE(56, 57, [
                            SPACE(56, 57),
                            ]),
                            WHITESPACE(57, 58, [
                            SPACE(57, 58),
                            ]),
                            WHITESPACE(58, 59, [
                            SPACE(58, 59),
                            ]),
                            WHITESPACE(59, 60, [
                            SPACE(59, 60),
                            ]),
                            WHITESPACE(60, 61, [
                            SPACE(60, 61),
                            ]),
                        ]),
                        ]),
                    ]),
                    WHITESPACE(62, 63, [
                        NEWLINE(62, 63),
                    ]),
                    WHITESPACE(63, 64, [
                        SPACE(63, 64),
                    ]),
                    WHITESPACE(64, 65, [
                        SPACE(64, 65),
                    ]),
                    WHITESPACE(65, 66, [
                        SPACE(65, 66),
                    ]),
                    WHITESPACE(66, 67, [
                        SPACE(66, 67),
                    ]),
                    ]),
                ]),
                ]),
                WHITESPACE(68, 69, [
                NEWLINE(68, 69),
                ]),
            ])
        ]
    }
}
