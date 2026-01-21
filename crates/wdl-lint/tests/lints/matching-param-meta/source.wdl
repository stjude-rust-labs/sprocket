#@ except: MetaDescription, InputName, RequirementsSection
#@ except: ParameterDescription

## This is a test for checking for missing and extraneous entries
## in a `parameter_meta` section, and for ensuring that
## the order is the same as `input` section.

version 1.3

# This workflow has both an extraneous and missing entry
# in the `parameter_meta` section
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

# This task only has a missing entry in the `parameter_meta` section
task foo {
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
    }

    input {
        String matching
        String does_not_exist
    }

    command <<<>>>

    output {}
}

# This task only has an extraneous entry in the `parameter_meta` section
task bar {
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
        does_not_exist: "this should not be here"
    }

    input {
        String matching
    }

    command <<<>>>

    output {}
}

# Task with out-of-order parameter_meta
task baz {
    meta {}

    parameter_meta {
        second: "This should be second"
        first: "This should be first"
    }

    input {
        # This should warn about incorrect ordering
        String first
        String second
    }

    command <<<>>>

    output {}
}
