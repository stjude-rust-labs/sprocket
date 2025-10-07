#@ except: DescriptionMissing, RuntimeSectionKeys, KnownRules
#@ except: MatchingOutputMeta, MetaDescription, ParameterMetaMatched
#@ except: ExpectedRuntimeKeys, PreambleCommentPlacement, EndingNewline
#@ except: PreambleFormatted, Whitespace

## Test that CallInputKeyword does NOT trigger for WDL 1.1.
## The `input:` keyword is required in version 1.1.

version 1.1

task example {
    meta {}

    parameter_meta {}

    input {
        String name
    }
                                   
    command <<<
        echo "~{name}"
    >>>

    output {
        String result = read_string(stdout())
    }

    runtime {}
}

workflow test {
    meta {}

    ## Should NOT trigger - version 1.1 requires `input:` keyword
    call example { input: name = "test" }

    output {}
}