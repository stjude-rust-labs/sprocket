## This is a test of having shellcheck style lints

#@ except: BashSetSyntax, EmptyOutputs, ExpectedRuntimeKeys, HereDocCommands
#@ except: MetaDescription, ParameterMetaMatched

version 1.1

task test1 {
    meta {}

    parameter_meta {}

    input {
        Int placeholder
    }

    command <<<
        [[ ]]
        [ true ]
    >>>

    output {}

    runtime {}
}

task test2 {
    meta {}

    parameter_meta {}

    input {
        Int placeholder
    }

    command {
        [[ ]]
        [ true ]
    }

    output {}

    runtime {}
}
