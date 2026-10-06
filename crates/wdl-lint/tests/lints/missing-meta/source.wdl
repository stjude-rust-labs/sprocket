#@ except: EmptyOutputs, BashSetSyntax, RuntimeSection

version 1.0

task test {
    parameter_meta {}

    input {}

    command <<<>>>
}

## This doc comment suppresses the lint
task documented {
    command <<<>>>
}

# This should not have diagnostics for <= 1.2
struct Test {
    String x
}
