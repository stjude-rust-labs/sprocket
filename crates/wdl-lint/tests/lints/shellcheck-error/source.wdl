## This is a test of having shellcheck error lints

#@ except: BashSetSyntax, EmptyOutputs, UnknownRuntimeKeys, DeprecatedRuntimeKey, RecommendedRuntimeKeys, HereDocCommands
#@ except: MetaDescription, MissingParameterMeta, ExtraneousParameterMeta, ParameterMetaOrder

version 1.1

task test1 {
    meta {}

    parameter_meta {}

    input {
        Int placeholder
    }

    command <<<
        somecommand.py [[ -f $broken_test]]
        if [ -f "$broken"]
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
        somecommand.py [[ -f $broken_test]]
        if [ -f "$broken"]
    }

    output {}

    runtime {}
}
