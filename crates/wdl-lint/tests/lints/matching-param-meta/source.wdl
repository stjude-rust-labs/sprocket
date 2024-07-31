#@ except: BlankLinesBetweenElements, DescriptionMissing, LineWidth, RuntimeSectionKeys, SectionOrdering, TrailingComma
## This is a test for checking for missing and extraneous entries
## in a `parameter_meta` section.

version 1.1

task t {
    meta {}
    input {
        String matching
        String does_not_exist
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

    runtime {}
    command <<<>>>
    output {}
}

workflow w {
    meta {}
    output {}
    input {
        String matching
        String does_not_exist
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
