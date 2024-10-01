#@ except: DescriptionMissing, RuntimeSectionKeys

## This is a test for checking for missing and extraneous entries
## in a `parameter_meta` section.

version 1.1

task t {
    meta {}

    parameter_meta {
        matching: {
            help: "a matching parameter!",
            foo: {
                bar: {
                    does_not_exist: "this should not suppress a missing input lint"
                },
            },
        }
        extra: "this should not be here"
    }

    input {
        String matching
        String does_not_exist
    }

    command <<<>>>

    output {}

    runtime {}
}

workflow w {
    meta {}

    parameter_meta {
        matching: {
            help: "a matching parameter!",
            foo: {
                bar: {
                    does_not_exist: "this should not suppress a missing input lint"
                },
            },
        }
        extra: "this should not be here"
    }

    input {
        String matching
        String does_not_exist
    }

    output {}
}
