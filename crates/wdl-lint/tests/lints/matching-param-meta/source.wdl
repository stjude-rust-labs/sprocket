## This is a test for checking for missing and extraneous entries
## in a `parameter_meta` section, and for ensuring that
## the order is the same as `input` section.

#@ except: BashSetSyntax, EmptyOutputs, InputName, MetaDescription
#@ except: RequirementsSection, EmptyOutputs, MetaSections, SectionOrdering

version 1.3

# This workflow has both an extraneous and missing entry
# in the `parameter_meta` section
workflow w {
    parameter_meta {
        matching: {
            description: "a matching parameter!",
            foo: {
                bar: {
                    does_not_exist: "this should not suppress a missing input lint",
                },
            },
        }
        extra: "this should not be here"
    }

    input {
        String matching
        String does_not_exist
    }
}

# This task only has a missing entry in the `parameter_meta` section
task foo {
    parameter_meta {
        matching: {
            description: "a matching parameter!",
            foo: {
                bar: {
                    does_not_exist: "this should not suppress a missing input lint",
                },
            },
        }
    }

    input {
        String matching
        String does_not_exist
    }

    command <<<>>>
}

# This task only has an extraneous entry in the `parameter_meta` section
task bar {
    parameter_meta {
        matching: {
            description: "a matching parameter!",
            foo: {
                bar: {
                    does_not_exist: "this should not suppress a missing input lint",
                },
            },
        }
        does_not_exist: "this should not be here"
    }

    input {
        String matching
    }

    command <<<>>>
}

# Task with out-of-order parameter_meta
task baz {
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
}

# Allow mixing in doc comments
task qux {
    input {
        String first
        ## `second` gets doc comments
        String second
    }

    parameter_meta {
        first: "`first` is documented with parameter_meta"
    }

    command <<<>>>
}

# Make sure ordering still works, ignoring doc comments
task quux {
    input {
        String first
        ## `second` gets doc comments
        String second
        String third
    }

    parameter_meta {
        third: "`third` also gets a parameter_meta entry"
        first: "`first` is documented with parameter_meta"
    }

    command <<<>>>
}

task corge {
    input {
        ## `first` should change the message for `second` to suggest doc comments
        String first
        String second
        String third
    }

    parameter_meta {
        third: "`third` gets a parameter_meta entry"
    }

    command <<<>>>
}
