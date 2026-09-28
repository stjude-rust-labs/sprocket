## This is a test of the `HereDocCommands` lint

#@ except: BashSetSyntax, EmptyOutputs, UnknownRuntimeKeys, DeprecatedRuntimeKey, RecommendedRuntimeKeys
#@ except: MetaDescription

version 1.1

task bad {
    meta {}

    parameter_meta {}

    command {
        echo "Hello, World!"
    }

    output {}

    runtime {}
}

task good {
    meta {}

    parameter_meta {}

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    runtime {}
}
