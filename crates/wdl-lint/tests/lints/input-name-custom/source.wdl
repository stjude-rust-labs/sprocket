#@ except: BashSetSyntax

version 1.3

#@ except: DeprecatedRuntimeSection, EmptyOutputs, RequirementsSection, SnakeCase
task foo {
    meta {
        description: "This is a test of the configurable input name checks"
    }

    parameter_meta {
        f: "desc"
        ab: "desc"
        abc: "desc"
        abcd: "desc"
        abcde: "desc"
        inString: "desc"
        InString: "desc"
        input_string: "desc"
        Input_string: "desc"
        in_string: "desc"
        inA: "desc"
        invalid: "desc"
    }

    input {
        File f
        Int ab
        Int abc
        Int abcd
        Int abcde
        String inString
        String InString
        String input_string
        String Input_string
        String in_string
        String inA
        String invalid
    }

    command <<<>>>

    output {}

    runtime {}
}
