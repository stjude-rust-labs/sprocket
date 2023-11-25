use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

#[test]
fn it_fails_to_parse_an_empty_literal() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::parameter_metadata,
        positives: vec![Rule::parameter_metadata],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_an_empty_parameter_metadata_object() {
    parses_to! {
        parser: WdlParser,
        input: "parameter_meta {}",
        rule: Rule::parameter_metadata,
        tokens: [
            // `parameter_meta {}`
            parameter_metadata(0, 17, [
                WHITESPACE(14, 15, [
                    SPACE(14, 15),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_an_empty_parameter_metadata_object_with_spaces() {
    parses_to! {
        parser: WdlParser,
        input: "parameter_meta   {   }",
        rule: Rule::parameter_metadata,
        tokens: [
            // `parameter_meta   {   }`
            parameter_metadata(0, 22, [
                WHITESPACE(14, 15, [
                    SPACE(14, 15),
                ]),
                WHITESPACE(15, 16, [
                    SPACE(15, 16),
                ]),
                WHITESPACE(16, 17, [
                    SPACE(16, 17),
                ]),
                WHITESPACE(18, 19, [
                    SPACE(18, 19),
                ]),
                WHITESPACE(19, 20, [
                    SPACE(19, 20),
                ]),
                WHITESPACE(20, 21, [
                    SPACE(20, 21),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_parameter_metadata_object_with_keys() {
    parses_to! {
        parser: WdlParser,
        input: r#"parameter_meta { hello: "world" foo: null }"#,
        rule: Rule::parameter_metadata,
        tokens: [
            // `parameter_meta { hello: "world" foo: null }`
            parameter_metadata(0, 43, [
                WHITESPACE(14, 15, [
                    SPACE(14, 15),
                ]),
                WHITESPACE(16, 17, [
                    SPACE(16, 17),
                ]),
                // `hello: "world"`
                metadata_kv(17, 31, [
                    // `hello`
                    metadata_key(17, 22, [
                        // `hello`
                        singular_identifier(17, 22),
                    ]),
                    WHITESPACE(23, 24, [
                        SPACE(23, 24),
                    ]),
                    // `"world"`
                    metadata_value(24, 31, [
                        // `"world"`
                        string(24, 31, [
                            // `"`
                            double_quote(24, 25),
                            // `world`
                            string_inner(25, 30, [
                                // `world`
                                string_literal_contents(25, 30),
                            ]),
                        ]),
                    ]),
                ]),
                WHITESPACE(31, 32, [
                    SPACE(31, 32),
                ]),
                // `foo: null`
                metadata_kv(32, 41, [
                    // `foo`
                    metadata_key(32, 35, [
                        // `foo`
                        singular_identifier(32, 35),
                    ]),
                    WHITESPACE(36, 37, [
                        SPACE(36, 37),
                    ]),
                    // `null`
                    metadata_value(37, 41, [
                        // `null`
                        null(37, 41),
                    ]),
                ]),
                WHITESPACE(41, 42, [
                    SPACE(41, 42),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_multiline_parameter_metadata_object_with_keys() {
    parses_to! {
        parser: WdlParser,
        input: r#"parameter_meta {
    hello: "world"
    foo: null
}"#,
        rule: Rule::parameter_metadata,
        tokens: [
            // `parameter_meta { hello: "world" foo: null }`
            parameter_metadata(0, 51, [
                WHITESPACE(14, 15, [
                    SPACE(14, 15),
                ]),
                WHITESPACE(16, 17, [
                    NEWLINE(16, 17),
                ]),
                WHITESPACE(17, 18, [
                    SPACE(17, 18),
                ]),
                WHITESPACE(18, 19, [
                    SPACE(18, 19),
                ]),
                WHITESPACE(19, 20, [
                    SPACE(19, 20),
                ]),
                WHITESPACE(20, 21, [
                    SPACE(20, 21),
                ]),
                // `hello: "world"`
                metadata_kv(21, 35, [
                    // `hello`
                    metadata_key(21, 26, [
                        // `hello`
                        singular_identifier(21, 26),
                    ]),
                    WHITESPACE(27, 28, [
                        SPACE(27, 28),
                    ]),
                    // `"world"`
                    metadata_value(28, 35, [
                        // `"world"`
                        string(28, 35, [
                            // `"`
                            double_quote(28, 29),
                            // `world`
                            string_inner(29, 34, [
                                // `world`
                                string_literal_contents(29, 34),
                            ]),
                        ]),
                    ]),
                ]),
                WHITESPACE(35, 36, [
                    NEWLINE(35, 36),
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
                // `foo: null`
                metadata_kv(40, 49, [
                    // `foo`
                    metadata_key(40, 43, [
                        // `foo`
                        singular_identifier(40, 43),
                    ]),
                    WHITESPACE(44, 45, [
                        SPACE(44, 45),
                    ]),
                    // `null`
                    metadata_value(45, 49, [
                        // `null`
                        null(45, 49),
                    ]),
                ]),
                WHITESPACE(49, 50, [
                    NEWLINE(49, 50),
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
fn it_fails_to_parse_a_parameter_metadata_with_top_level_commas() {
    fails_with! {
        parser: WdlParser,
        input: r#"parameter_meta {
    hello: "world",
    foo: null
}"#,
        rule: Rule::parameter_metadata,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT],
        negatives: vec![Rule::COMMA],
        pos: 35
    }
}

#[test]
fn it_fails_to_parse_a_parameter_metadata_without_commas_within_a_metadata_object() {
    fails_with! {
        parser: WdlParser,
        input: r#"parameter_meta {
    hello: {
        foo: "bar"
        baz: "quux"
    }
}"#,
        rule: Rule::parameter_metadata,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT, Rule::COMMA],
        negatives: vec![],
        pos: 57
    }
}

#[test]
fn it_fails_to_parse_a_parameter_metadata_without_commas_within_a_metadata_array() {
    fails_with! {
        parser: WdlParser,
        input: r#"parameter_meta {
    hello: {
        foo: [
            "bar"
            "quux"
        ]
    }
}"#,
        rule: Rule::parameter_metadata,
        positives: vec![Rule::WHITESPACE, Rule::COMMENT, Rule::COMMA],
        negatives: vec![],
        pos: 75
    }
}

#[test]
fn it_successfully_parse_a_parameter_metadata_with_commas_within_a_metadata_object() {
    parses_to! {
        parser: WdlParser,
        input: r#"parameter_meta {
    hello: {
        foo: "bar",
        baz: "quux"
    }
}"#,
        rule: Rule::parameter_metadata,
        tokens: [
            // `parameter_meta { hello: { foo: "bar", baz: "quux" } }`
            parameter_metadata(0, 77, [
                WHITESPACE(14, 15, [
                    SPACE(14, 15),
                ]),
                WHITESPACE(16, 17, [
                    NEWLINE(16, 17),
                ]),
                WHITESPACE(17, 18, [
                    SPACE(17, 18),
                ]),
                WHITESPACE(18, 19, [
                    SPACE(18, 19),
                ]),
                WHITESPACE(19, 20, [
                    SPACE(19, 20),
                ]),
                WHITESPACE(20, 21, [
                    SPACE(20, 21),
                ]),
                // `hello: { foo: "bar", baz: "quux" }`
                metadata_kv(21, 75, [
                // `hello`
                metadata_key(21, 26, [
                    // `hello`
                    singular_identifier(21, 26),
                ]),
                WHITESPACE(27, 28, [
                    SPACE(27, 28),
                ]),
                // `{ foo: "bar", baz: "quux" }`
                metadata_value(28, 75, [
                    // `{ foo: "bar", baz: "quux" }`
                    metadata_object(28, 75, [
                    WHITESPACE(29, 30, [
                        NEWLINE(29, 30),
                    ]),
                    WHITESPACE(30, 31, [
                        SPACE(30, 31),
                    ]),
                    WHITESPACE(31, 32, [
                        SPACE(31, 32),
                    ]),
                    WHITESPACE(32, 33, [
                        SPACE(32, 33),
                    ]),
                    WHITESPACE(33, 34, [
                        SPACE(33, 34),
                    ]),
                    WHITESPACE(34, 35, [
                        SPACE(34, 35),
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
                    // `foo: "bar"`
                    metadata_kv(38, 48, [
                        // `foo`
                        metadata_key(38, 41, [
                        // `foo`
                        singular_identifier(38, 41),
                        ]),
                        WHITESPACE(42, 43, [
                        SPACE(42, 43),
                        ]),
                        // `"bar"`
                        metadata_value(43, 48, [
                        // `"bar"`
                        string(43, 48, [
                            // `"`
                            double_quote(43, 44),
                            // `bar`
                            string_inner(44, 47, [
                            // `bar`
                            string_literal_contents(44, 47),
                            ]),
                        ]),
                        ]),
                    ]),
                    // `,`
                    COMMA(48, 49),
                    WHITESPACE(49, 50, [
                        NEWLINE(49, 50),
                    ]),
                    WHITESPACE(50, 51, [
                        SPACE(50, 51),
                    ]),
                    WHITESPACE(51, 52, [
                        SPACE(51, 52),
                    ]),
                    WHITESPACE(52, 53, [
                        SPACE(52, 53),
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
                    // `baz: "quux"`
                    metadata_kv(58, 69, [
                        // `baz`
                        metadata_key(58, 61, [
                        // `baz`
                        singular_identifier(58, 61),
                        ]),
                        WHITESPACE(62, 63, [
                        SPACE(62, 63),
                        ]),
                        // `"quux"`
                        metadata_value(63, 69, [
                        // `"quux"`
                        string(63, 69, [
                            // `"`
                            double_quote(63, 64),
                            // `quux`
                            string_inner(64, 68, [
                            // `quux`
                            string_literal_contents(64, 68),
                            ]),
                        ]),
                        ]),
                    ]),
                    WHITESPACE(69, 70, [
                        NEWLINE(69, 70),
                    ]),
                    WHITESPACE(70, 71, [
                        SPACE(70, 71),
                    ]),
                    WHITESPACE(71, 72, [
                        SPACE(71, 72),
                    ]),
                    WHITESPACE(72, 73, [
                        SPACE(72, 73),
                    ]),
                    WHITESPACE(73, 74, [
                        SPACE(73, 74),
                    ]),
                    ]),
                ]),
                ]),
                WHITESPACE(75, 76, [
                NEWLINE(75, 76),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_parameter_metadata_with_commas_within_a_metadata_array() {
    parses_to! {
        parser: WdlParser,
        input: r#"parameter_meta {
    hello: {
        foo: [
            "bar",
            "quux"
        ]
    }
}"#,
        rule: Rule::parameter_metadata,
        tokens: [
            // `parameter_meta { hello: { foo: [ "bar", "quux" ] } }`
            parameter_metadata(0, 100, [
                WHITESPACE(14, 15, [
                SPACE(14, 15),
                ]),
                WHITESPACE(16, 17, [
                NEWLINE(16, 17),
                ]),
                WHITESPACE(17, 18, [
                SPACE(17, 18),
                ]),
                WHITESPACE(18, 19, [
                SPACE(18, 19),
                ]),
                WHITESPACE(19, 20, [
                SPACE(19, 20),
                ]),
                WHITESPACE(20, 21, [
                SPACE(20, 21),
                ]),
                // `hello: { foo: [ "bar", "quux" ] }`
                metadata_kv(21, 98, [
                // `hello`
                metadata_key(21, 26, [
                    // `hello`
                    singular_identifier(21, 26),
                ]),
                WHITESPACE(27, 28, [
                    SPACE(27, 28),
                ]),
                // `{ foo: [ "bar", "quux" ] }`
                metadata_value(28, 98, [
                    // `{ foo: [ "bar", "quux" ] }`
                    metadata_object(28, 98, [
                    WHITESPACE(29, 30, [
                        NEWLINE(29, 30),
                    ]),
                    WHITESPACE(30, 31, [
                        SPACE(30, 31),
                    ]),
                    WHITESPACE(31, 32, [
                        SPACE(31, 32),
                    ]),
                    WHITESPACE(32, 33, [
                        SPACE(32, 33),
                    ]),
                    WHITESPACE(33, 34, [
                        SPACE(33, 34),
                    ]),
                    WHITESPACE(34, 35, [
                        SPACE(34, 35),
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
                    // `foo: [ "bar", "quux" ]`
                    metadata_kv(38, 92, [
                        // `foo`
                        metadata_key(38, 41, [
                        // `foo`
                        singular_identifier(38, 41),
                        ]),
                        WHITESPACE(42, 43, [
                        SPACE(42, 43),
                        ]),
                        // `[ "bar", "quux" ]`
                        metadata_value(43, 92, [
                        // `[ "bar", "quux" ]`
                        metadata_array(43, 92, [
                            WHITESPACE(44, 45, [
                            NEWLINE(44, 45),
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
                            WHITESPACE(48, 49, [
                            SPACE(48, 49),
                            ]),
                            WHITESPACE(49, 50, [
                            SPACE(49, 50),
                            ]),
                            WHITESPACE(50, 51, [
                            SPACE(50, 51),
                            ]),
                            WHITESPACE(51, 52, [
                            SPACE(51, 52),
                            ]),
                            WHITESPACE(52, 53, [
                            SPACE(52, 53),
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
                            // `"bar"`
                            metadata_value(57, 62, [
                            // `"bar"`
                            string(57, 62, [
                                // `"`
                                double_quote(57, 58),
                                // `bar`
                                string_inner(58, 61, [
                                // `bar`
                                string_literal_contents(58, 61),
                                ]),
                            ]),
                            ]),
                            // `,`
                            COMMA(62, 63),
                            WHITESPACE(63, 64, [
                            NEWLINE(63, 64),
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
                            WHITESPACE(67, 68, [
                            SPACE(67, 68),
                            ]),
                            WHITESPACE(68, 69, [
                            SPACE(68, 69),
                            ]),
                            WHITESPACE(69, 70, [
                            SPACE(69, 70),
                            ]),
                            WHITESPACE(70, 71, [
                            SPACE(70, 71),
                            ]),
                            WHITESPACE(71, 72, [
                            SPACE(71, 72),
                            ]),
                            WHITESPACE(72, 73, [
                            SPACE(72, 73),
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
                            // `"quux"`
                            metadata_value(76, 82, [
                            // `"quux"`
                            string(76, 82, [
                                // `"`
                                double_quote(76, 77),
                                // `quux`
                                string_inner(77, 81, [
                                // `quux`
                                string_literal_contents(77, 81),
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
                            WHITESPACE(87, 88, [
                            SPACE(87, 88),
                            ]),
                            WHITESPACE(88, 89, [
                            SPACE(88, 89),
                            ]),
                            WHITESPACE(89, 90, [
                            SPACE(89, 90),
                            ]),
                            WHITESPACE(90, 91, [
                            SPACE(90, 91),
                            ]),
                        ]),
                        ]),
                    ]),
                    WHITESPACE(92, 93, [
                        NEWLINE(92, 93),
                    ]),
                    WHITESPACE(93, 94, [
                        SPACE(93, 94),
                    ]),
                    WHITESPACE(94, 95, [
                        SPACE(94, 95),
                    ]),
                    WHITESPACE(95, 96, [
                        SPACE(95, 96),
                    ]),
                    WHITESPACE(96, 97, [
                        SPACE(96, 97),
                    ]),
                    ]),
                ]),
                ]),
                WHITESPACE(98, 99, [
                NEWLINE(98, 99),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parse_a_parameter_metadata_with_commas_within_a_metadata_object_with_a_trailing_comma(
) {
    parses_to! {
        parser: WdlParser,
        input: r#"parameter_meta {
    hello: {
        foo: "bar",
        baz: "quux",
    }
}"#,
        rule: Rule::parameter_metadata,
        tokens: [
            // `parameter_meta { hello: { foo: "bar", baz: "quux", } }`
            parameter_metadata(0, 78, [
                WHITESPACE(14, 15, [
                SPACE(14, 15),
                ]),
                WHITESPACE(16, 17, [
                NEWLINE(16, 17),
                ]),
                WHITESPACE(17, 18, [
                SPACE(17, 18),
                ]),
                WHITESPACE(18, 19, [
                SPACE(18, 19),
                ]),
                WHITESPACE(19, 20, [
                SPACE(19, 20),
                ]),
                WHITESPACE(20, 21, [
                SPACE(20, 21),
                ]),
                // `hello: { foo: "bar", baz: "quux", }`
                metadata_kv(21, 76, [
                // `hello`
                metadata_key(21, 26, [
                    // `hello`
                    singular_identifier(21, 26),
                ]),
                WHITESPACE(27, 28, [
                    SPACE(27, 28),
                ]),
                // `{ foo: "bar", baz: "quux", }`
                metadata_value(28, 76, [
                    // `{ foo: "bar", baz: "quux", }`
                    metadata_object(28, 76, [
                    WHITESPACE(29, 30, [
                        NEWLINE(29, 30),
                    ]),
                    WHITESPACE(30, 31, [
                        SPACE(30, 31),
                    ]),
                    WHITESPACE(31, 32, [
                        SPACE(31, 32),
                    ]),
                    WHITESPACE(32, 33, [
                        SPACE(32, 33),
                    ]),
                    WHITESPACE(33, 34, [
                        SPACE(33, 34),
                    ]),
                    WHITESPACE(34, 35, [
                        SPACE(34, 35),
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
                    // `foo: "bar"`
                    metadata_kv(38, 48, [
                        // `foo`
                        metadata_key(38, 41, [
                        // `foo`
                        singular_identifier(38, 41),
                        ]),
                        WHITESPACE(42, 43, [
                        SPACE(42, 43),
                        ]),
                        // `"bar"`
                        metadata_value(43, 48, [
                        // `"bar"`
                        string(43, 48, [
                            // `"`
                            double_quote(43, 44),
                            // `bar`
                            string_inner(44, 47, [
                            // `bar`
                            string_literal_contents(44, 47),
                            ]),
                        ]),
                        ]),
                    ]),
                    // `,`
                    COMMA(48, 49),
                    WHITESPACE(49, 50, [
                        NEWLINE(49, 50),
                    ]),
                    WHITESPACE(50, 51, [
                        SPACE(50, 51),
                    ]),
                    WHITESPACE(51, 52, [
                        SPACE(51, 52),
                    ]),
                    WHITESPACE(52, 53, [
                        SPACE(52, 53),
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
                    // `baz: "quux"`
                    metadata_kv(58, 69, [
                        // `baz`
                        metadata_key(58, 61, [
                        // `baz`
                        singular_identifier(58, 61),
                        ]),
                        WHITESPACE(62, 63, [
                        SPACE(62, 63),
                        ]),
                        // `"quux"`
                        metadata_value(63, 69, [
                        // `"quux"`
                        string(63, 69, [
                            // `"`
                            double_quote(63, 64),
                            // `quux`
                            string_inner(64, 68, [
                            // `quux`
                            string_literal_contents(64, 68),
                            ]),
                        ]),
                        ]),
                    ]),
                    // `,`
                    COMMA(69, 70),
                    WHITESPACE(70, 71, [
                        NEWLINE(70, 71),
                    ]),
                    WHITESPACE(71, 72, [
                        SPACE(71, 72),
                    ]),
                    WHITESPACE(72, 73, [
                        SPACE(72, 73),
                    ]),
                    WHITESPACE(73, 74, [
                        SPACE(73, 74),
                    ]),
                    WHITESPACE(74, 75, [
                        SPACE(74, 75),
                    ]),
                    ]),
                ]),
                ]),
                WHITESPACE(76, 77, [
                NEWLINE(76, 77),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_parameter_metadata_with_commas_within_a_metadata_array_with_a_trailing_comma(
) {
    parses_to! {
        parser: WdlParser,
        input: r#"parameter_meta {
    hello: {
        foo: [
            "bar",
            "quux",
        ]
    }
}"#,
        rule: Rule::parameter_metadata,
        tokens: [
            // `parameter_meta { hello: { foo: [ "bar", "quux", ] } }`
            parameter_metadata(0, 101, [
                WHITESPACE(14, 15, [
                SPACE(14, 15),
                ]),
                WHITESPACE(16, 17, [
                NEWLINE(16, 17),
                ]),
                WHITESPACE(17, 18, [
                SPACE(17, 18),
                ]),
                WHITESPACE(18, 19, [
                SPACE(18, 19),
                ]),
                WHITESPACE(19, 20, [
                SPACE(19, 20),
                ]),
                WHITESPACE(20, 21, [
                SPACE(20, 21),
                ]),
                // `hello: { foo: [ "bar", "quux", ] }`
                metadata_kv(21, 99, [
                // `hello`
                metadata_key(21, 26, [
                    // `hello`
                    singular_identifier(21, 26),
                ]),
                WHITESPACE(27, 28, [
                    SPACE(27, 28),
                ]),
                // `{ foo: [ "bar", "quux", ] }`
                metadata_value(28, 99, [
                    // `{ foo: [ "bar", "quux", ] }`
                    metadata_object(28, 99, [
                    WHITESPACE(29, 30, [
                        NEWLINE(29, 30),
                    ]),
                    WHITESPACE(30, 31, [
                        SPACE(30, 31),
                    ]),
                    WHITESPACE(31, 32, [
                        SPACE(31, 32),
                    ]),
                    WHITESPACE(32, 33, [
                        SPACE(32, 33),
                    ]),
                    WHITESPACE(33, 34, [
                        SPACE(33, 34),
                    ]),
                    WHITESPACE(34, 35, [
                        SPACE(34, 35),
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
                    // `foo: [ "bar", "quux", ]`
                    metadata_kv(38, 93, [
                        // `foo`
                        metadata_key(38, 41, [
                        // `foo`
                        singular_identifier(38, 41),
                        ]),
                        WHITESPACE(42, 43, [
                        SPACE(42, 43),
                        ]),
                        // `[ "bar", "quux", ]`
                        metadata_value(43, 93, [
                        // `[ "bar", "quux", ]`
                        metadata_array(43, 93, [
                            WHITESPACE(44, 45, [
                            NEWLINE(44, 45),
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
                            WHITESPACE(48, 49, [
                            SPACE(48, 49),
                            ]),
                            WHITESPACE(49, 50, [
                            SPACE(49, 50),
                            ]),
                            WHITESPACE(50, 51, [
                            SPACE(50, 51),
                            ]),
                            WHITESPACE(51, 52, [
                            SPACE(51, 52),
                            ]),
                            WHITESPACE(52, 53, [
                            SPACE(52, 53),
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
                            // `"bar"`
                            metadata_value(57, 62, [
                            // `"bar"`
                            string(57, 62, [
                                // `"`
                                double_quote(57, 58),
                                // `bar`
                                string_inner(58, 61, [
                                // `bar`
                                string_literal_contents(58, 61),
                                ]),
                            ]),
                            ]),
                            // `,`
                            COMMA(62, 63),
                            WHITESPACE(63, 64, [
                            NEWLINE(63, 64),
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
                            WHITESPACE(67, 68, [
                            SPACE(67, 68),
                            ]),
                            WHITESPACE(68, 69, [
                            SPACE(68, 69),
                            ]),
                            WHITESPACE(69, 70, [
                            SPACE(69, 70),
                            ]),
                            WHITESPACE(70, 71, [
                            SPACE(70, 71),
                            ]),
                            WHITESPACE(71, 72, [
                            SPACE(71, 72),
                            ]),
                            WHITESPACE(72, 73, [
                            SPACE(72, 73),
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
                            // `"quux"`
                            metadata_value(76, 82, [
                            // `"quux"`
                            string(76, 82, [
                                // `"`
                                double_quote(76, 77),
                                // `quux`
                                string_inner(77, 81, [
                                // `quux`
                                string_literal_contents(77, 81),
                                ]),
                            ]),
                            ]),
                            // `,`
                            COMMA(82, 83),
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
                            WHITESPACE(88, 89, [
                            SPACE(88, 89),
                            ]),
                            WHITESPACE(89, 90, [
                            SPACE(89, 90),
                            ]),
                            WHITESPACE(90, 91, [
                            SPACE(90, 91),
                            ]),
                            WHITESPACE(91, 92, [
                            SPACE(91, 92),
                            ]),
                        ]),
                        ]),
                    ]),
                    WHITESPACE(93, 94, [
                        NEWLINE(93, 94),
                    ]),
                    WHITESPACE(94, 95, [
                        SPACE(94, 95),
                    ]),
                    WHITESPACE(95, 96, [
                        SPACE(95, 96),
                    ]),
                    WHITESPACE(96, 97, [
                        SPACE(96, 97),
                    ]),
                    WHITESPACE(97, 98, [
                        SPACE(97, 98),
                    ]),
                    ]),
                ]),
                ]),
                WHITESPACE(99, 100, [
                NEWLINE(99, 100),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parse_a_metadata_object_with_a_single_element_and_no_commas() {
    parses_to! {
        parser: WdlParser,
        input: r#"parameter_meta {
    hello: {
        foo: "bar"
    }
}"#,
        rule: Rule::parameter_metadata,
        tokens: [
            // `parameter_meta { hello: { foo: "bar" } }`
            parameter_metadata(0, 56, [
                WHITESPACE(14, 15, [
                SPACE(14, 15),
                ]),
                WHITESPACE(16, 17, [
                NEWLINE(16, 17),
                ]),
                WHITESPACE(17, 18, [
                SPACE(17, 18),
                ]),
                WHITESPACE(18, 19, [
                SPACE(18, 19),
                ]),
                WHITESPACE(19, 20, [
                SPACE(19, 20),
                ]),
                WHITESPACE(20, 21, [
                SPACE(20, 21),
                ]),
                // `hello: { foo: "bar" }`
                metadata_kv(21, 54, [
                // `hello`
                metadata_key(21, 26, [
                    // `hello`
                    singular_identifier(21, 26),
                ]),
                WHITESPACE(27, 28, [
                    SPACE(27, 28),
                ]),
                // `{ foo: "bar" }`
                metadata_value(28, 54, [
                    // `{ foo: "bar" }`
                    metadata_object(28, 54, [
                    WHITESPACE(29, 30, [
                        NEWLINE(29, 30),
                    ]),
                    WHITESPACE(30, 31, [
                        SPACE(30, 31),
                    ]),
                    WHITESPACE(31, 32, [
                        SPACE(31, 32),
                    ]),
                    WHITESPACE(32, 33, [
                        SPACE(32, 33),
                    ]),
                    WHITESPACE(33, 34, [
                        SPACE(33, 34),
                    ]),
                    WHITESPACE(34, 35, [
                        SPACE(34, 35),
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
                    // `foo: "bar"`
                    metadata_kv(38, 48, [
                        // `foo`
                        metadata_key(38, 41, [
                        // `foo`
                        singular_identifier(38, 41),
                        ]),
                        WHITESPACE(42, 43, [
                        SPACE(42, 43),
                        ]),
                        // `"bar"`
                        metadata_value(43, 48, [
                        // `"bar"`
                        string(43, 48, [
                            // `"`
                            double_quote(43, 44),
                            // `bar`
                            string_inner(44, 47, [
                            // `bar`
                            string_literal_contents(44, 47),
                            ]),
                        ]),
                        ]),
                    ]),
                    WHITESPACE(48, 49, [
                        NEWLINE(48, 49),
                    ]),
                    WHITESPACE(49, 50, [
                        SPACE(49, 50),
                    ]),
                    WHITESPACE(50, 51, [
                        SPACE(50, 51),
                    ]),
                    WHITESPACE(51, 52, [
                        SPACE(51, 52),
                    ]),
                    WHITESPACE(52, 53, [
                        SPACE(52, 53),
                    ]),
                    ]),
                ]),
                ]),
                WHITESPACE(54, 55, [
                NEWLINE(54, 55),
                ]),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_a_metadata_array_with_a_single_element_and_no_commas() {
    parses_to! {
        parser: WdlParser,
        input: r#"parameter_meta {
    hello: {
        foo: [
            "bar"
        ]
    }
}"#,
        rule: Rule::parameter_metadata,
        tokens: [
            // `parameter_meta { hello: { foo: [ "bar" ] } }`
            parameter_metadata(0, 80, [
                WHITESPACE(14, 15, [
                SPACE(14, 15),
                ]),
                WHITESPACE(16, 17, [
                NEWLINE(16, 17),
                ]),
                WHITESPACE(17, 18, [
                SPACE(17, 18),
                ]),
                WHITESPACE(18, 19, [
                SPACE(18, 19),
                ]),
                WHITESPACE(19, 20, [
                SPACE(19, 20),
                ]),
                WHITESPACE(20, 21, [
                SPACE(20, 21),
                ]),
                // `hello: { foo: [ "bar" ] }`
                metadata_kv(21, 78, [
                // `hello`
                metadata_key(21, 26, [
                    // `hello`
                    singular_identifier(21, 26),
                ]),
                WHITESPACE(27, 28, [
                    SPACE(27, 28),
                ]),
                // `{ foo: [ "bar" ] }`
                metadata_value(28, 78, [
                    // `{ foo: [ "bar" ] }`
                    metadata_object(28, 78, [
                    WHITESPACE(29, 30, [
                        NEWLINE(29, 30),
                    ]),
                    WHITESPACE(30, 31, [
                        SPACE(30, 31),
                    ]),
                    WHITESPACE(31, 32, [
                        SPACE(31, 32),
                    ]),
                    WHITESPACE(32, 33, [
                        SPACE(32, 33),
                    ]),
                    WHITESPACE(33, 34, [
                        SPACE(33, 34),
                    ]),
                    WHITESPACE(34, 35, [
                        SPACE(34, 35),
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
                    // `foo: [ "bar" ]`
                    metadata_kv(38, 72, [
                        // `foo`
                        metadata_key(38, 41, [
                        // `foo`
                        singular_identifier(38, 41),
                        ]),
                        WHITESPACE(42, 43, [
                        SPACE(42, 43),
                        ]),
                        // `[ "bar" ]`
                        metadata_value(43, 72, [
                        // `[ "bar" ]`
                        metadata_array(43, 72, [
                            WHITESPACE(44, 45, [
                            NEWLINE(44, 45),
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
                            WHITESPACE(48, 49, [
                            SPACE(48, 49),
                            ]),
                            WHITESPACE(49, 50, [
                            SPACE(49, 50),
                            ]),
                            WHITESPACE(50, 51, [
                            SPACE(50, 51),
                            ]),
                            WHITESPACE(51, 52, [
                            SPACE(51, 52),
                            ]),
                            WHITESPACE(52, 53, [
                            SPACE(52, 53),
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
                            // `"bar"`
                            metadata_value(57, 62, [
                            // `"bar"`
                            string(57, 62, [
                                // `"`
                                double_quote(57, 58),
                                // `bar`
                                string_inner(58, 61, [
                                // `bar`
                                string_literal_contents(58, 61),
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
                            WHITESPACE(67, 68, [
                            SPACE(67, 68),
                            ]),
                            WHITESPACE(68, 69, [
                            SPACE(68, 69),
                            ]),
                            WHITESPACE(69, 70, [
                            SPACE(69, 70),
                            ]),
                            WHITESPACE(70, 71, [
                            SPACE(70, 71),
                            ]),
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
                    ]),
                ]),
                ]),
                WHITESPACE(78, 79, [
                NEWLINE(78, 79),
                ]),
            ])
        ]
    }
}
