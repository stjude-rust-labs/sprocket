use pest::consumes_to;
use pest::fails_with;
use pest::parses_to;

use crate::v1::Parser as WdlParser;
use crate::v1::Rule;

mod escaped;
mod hex;
mod octal;
mod unicode;

#[test]
fn it_fails_to_parse_an_empty_char_special() {
    fails_with! {
        parser: WdlParser,
        input: "",
        rule: Rule::char_special,
        positives: vec![Rule::char_special],
        negatives: vec![],
        pos: 0
    }
}

#[test]
fn it_successfully_parses_char_escaped() {
    parses_to! {
        parser: WdlParser,
        input: "\\\\",
        rule: Rule::char_special,
        tokens: [
            // `\\`
            char_special(0, 2, [
                // `\\`
                char_escaped(0, 2),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_char_hex() {
    parses_to! {
        parser: WdlParser,
        input: "\\xFF",
        rule: Rule::char_special,
        tokens: [
            // `\xFF`
            char_special(0, 4, [
                // `\xFF`
                char_hex(0, 4),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_char_octal() {
    parses_to! {
        parser: WdlParser,
        input: "\\123",
        rule: Rule::char_special,
        tokens: [
            // `\123`
            char_special(0, 4, [
                // `\123`
                char_octal(0, 4),
            ])
        ]
    }
}

#[test]
fn it_successfully_parses_char_unicode() {
    parses_to! {
        parser: WdlParser,
        input: "\\uFFFF",
        rule: Rule::char_special,
        tokens: [
            // `\uFFFF`
            char_special(0, 6, [
                // `\uFFFF`
                char_unicode(0, 6, [
                    // `\uFFFF`
                    char_unicode_four(0, 6),
                ]),
            ])
        ]
    }

    parses_to! {
        parser: WdlParser,
        input: "\\UFFFF",
        rule: Rule::char_special,
        tokens: [
            // `\UFFFF`
            char_special(0, 6, [
                // `\UFFFF`
                char_unicode(0, 6, [
                    // `\UFFFF`
                    char_unicode_four(0, 6),
                ]),
            ])
        ]
    }

    parses_to! {
        parser: WdlParser,
        input: "\\uFFFFFFFF",
        rule: Rule::char_special,
        tokens: [
            // `\uFFFFFFFF`
            char_special(0, 10, [
                // `\uFFFFFFFF`
                char_unicode(0, 10, [
                    // `\uFFFFFFFF`
                    char_unicode_eight(0, 10),
                ]),
            ])
        ]
    }

    parses_to! {
        parser: WdlParser,
        input: "\\UFFFFFFFF",
        rule: Rule::char_special,
        tokens: [
            // `\UFFFFFFFF`
            char_special(0, 10, [
                // `\UFFFFFFFF`
                char_unicode(0, 10, [
                    // `\UFFFFFFFF`
                    char_unicode_eight(0, 10),
                ]),
            ])
        ]
    }
}
