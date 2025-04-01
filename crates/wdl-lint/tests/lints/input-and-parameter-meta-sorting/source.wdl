#@ except: DescriptionMissing, DisallowedInputName, MissingRequirements

## This is a test related to the `InputSorting` and `MatchingParamMeta`
## diagnostic, specifically, it tests how they interact with each other.

version 1.2

# This should trigger a InputSorting diagnostic,
# but not a `MatchingParameterMeta` diagnostic
task input_sorting_test_1 {
    meta {}

    parameter_meta {
        b: "Another file input"
        p: "Array of non-optional strings"
        q: "Another array of non-optional strings"
        t: "File input"
        w: "Directory input"
    }

    input {
        File b
        Array[String]+ p
        Array[String]+ q
        File t
        Directory w
    }

    command <<<>>>

    output {}
}

# This should trigger both an InputSorting diagnostic
# as well as a `MatchingParameterMeta` diagnostic
task input_sorting_test_2 {
    meta {}

    parameter_meta {
        p: "Array of non-optional strings"
        w: "Directory input"
        b: "Another file input"
        q: "Another array of non-optional strings"
        t: "File input"
    }

    input {
        # Incorrect order for both input order and parameter_meta
        Directory w
        Array[String]+ p
        File t
        Array[String]+ q
        File b
    }

    command <<<>>>

    output {}
}
