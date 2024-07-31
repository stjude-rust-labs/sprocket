#@ except: BlankLinesBetweenElements, RuntimeSectionKeys, SectionOrdering

version 1.0

task test {
    runtime {}
    command <<<>>>
    input {}
    parameter_meta {}
    output {}
}

# This should not have diagnostics for <= 1.2
struct Test {
    String x
}
