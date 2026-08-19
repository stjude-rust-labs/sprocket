## This is a test of escape sequences in `meta` and `parameter_meta` sections.
##
## Escape sequences are not interpreted in metadata, so the shape of an escape
## sequence is not validated there; the rules about characters that must be
## escaped still apply.

version 1.3

struct Foo {
    meta {
        # Struct metadata uses the same nodes as task metadata.
        unknown_dot: "an unknown \. escape"
    }

    Int a
}

task t {
    meta {
        unknown_dot: "an unknown \. escape"
        unknown_underscore: "an unknown \_ escape"
        invalid_hex: "an invalid \x escape"
        invalid_short_unicode: "an invalid \u12 escape"
        invalid_octal: "an invalid \8 escape"

        nested_object: {
            unknown_dot: "an unknown \. escape",
            unknown_underscore: "an unknown \_ escape",
            invalid_hex: "an invalid \x escape",
            invalid_short_unicode: "an invalid \u12 escape",
            invalid_octal: "an invalid \8 escape"
        }

        nested_array: [
            "an unknown \. escape",
            "an unknown \_ escape",
            "an invalid \x escape",
            "an invalid \u12 escape",
            "an invalid \8 escape"
        ]

        # Characters that must be escaped are still checked in metadata.
        raw_tab: "this has a	tab"
        raw_newline: "this has a
            newline"
    }

    parameter_meta {
        newId: {description: "Assign ID on the fly (e.g. --set-id +'%CHROM\_%POS').", category: "advanced"}
    }

    input {
        String newId
    }

    # Outside of metadata, an unknown escape sequence is still an error.
    String regex = "\.bam$"

    command <<<>>>

    output {
        String out = regex + newId
    }
}
