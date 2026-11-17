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

# This struct should have diagnostics for missing doc comments
struct Test {
    String x
}
