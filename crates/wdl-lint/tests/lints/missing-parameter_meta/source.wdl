#@ except: BlankLinesBetweenElements, DescriptionMissing, SectionOrdering

version 1.0

workflow test {
    input {}
    output {}
    meta {}
}

# This should not have diagnostics for <= 1.2
struct Test {
    String x
}
