# This is a test of an unbound decl being present in an output section

version 1.1

task test {
    output {
        # This should be bound!
        String x
    }
}
