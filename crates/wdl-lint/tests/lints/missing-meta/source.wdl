version 1.0

task test {
    parameter_meta {}

    input {}

    command <<<>>>

    output {}

    #@ except: ExpectedRuntimeKeys
    runtime {}
}

# This should not have diagnostics for <= 1.2
struct Test {
    String x
}
