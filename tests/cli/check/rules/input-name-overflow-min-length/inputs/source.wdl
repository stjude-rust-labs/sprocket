#@ except: UnusedInput, SnakeCase, MetaSections, DeprecatedRuntimeSection, EmptyOutputs, RequirementsSection, BashSetSyntax

version 1.3

task foo {
    input {
        String abcd
        String inValue
    }

    command <<<>>>

    output {}

    runtime {}
}
