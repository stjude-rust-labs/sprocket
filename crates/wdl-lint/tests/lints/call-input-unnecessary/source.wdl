#@ except: PreambleFormatted, MatchingOutputMeta, MetaDescription, ParameterMetaMatched, RequirementsSection, PreambleCommentPlacement, TrailingComma, CallInputSpacing, LintDirectiveValid, EndingNewline

## This is a test of the CallInputUnnecessary rule for WDL 1.2+.
## The input: keyword is optional in version 1.2 and should be omitted.

version 1.2

task example1 {
    meta {}

    parameter_meta {}

    input {
        String name
        Int count = 5
    }

    command <<<
        echo "~{name}: ~{count}"
    >>>

    output {
        String result = read_string(stdout())
    }

    runtime {}
}

task example2 {
    meta {}

    parameter_meta {}

    input {
        String name
        Int count = 5
    }

    command <<<
        echo "~{name}: ~{count}"
    >>>

    output {
        String result = read_string(stdout())
    }

    runtime {}
}

workflow test {
    meta {}

    ## Should trigger diagnostic - unnecessary input: keyword
    call example1 { input:
        name = "test",
        count = 10
    }

    ## Should NOT trigger - no input: keyword (correct for v1.2)
    call example2 {
        name = "world",
        count = 15
    }

    output {}
}
