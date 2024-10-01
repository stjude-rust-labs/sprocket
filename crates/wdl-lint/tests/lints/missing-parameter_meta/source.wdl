version 1.0

workflow test {
    #@ except: DescriptionMissing
    meta {}

    input {}

    output {}
}

# This should not have diagnostics for <= 1.2
struct Test {
    String x
}
