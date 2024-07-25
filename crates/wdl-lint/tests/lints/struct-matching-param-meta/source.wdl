#@ except: TrailingComma
## This is a test for checking for missing and extraneous entries
## in a `parameter_meta` section for a struct.

version 1.2

struct Text {
    String matching
    String does_not_exist

    meta {
        description: "foo"
    }

    parameter_meta {
        matching: {
            help: "a matching parameter!",
            foo: {
                bar: {
                    does_not_exist: "this should not suppress a missing input lint"
                }
            }
        }
        extra: "this should not be here"
    }
}
