#@ except: UnusedInput, SnakeCase, MetaSections, DeprecatedRuntimeSection, EmptyOutputs, RequirementsSection, BashSetSyntax

version 1.4

task foo {
    input {
        String abcd
        String inValue
    }

    command <<<>>>

    output {}

    runtime {}
}
