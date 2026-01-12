version 1.3

struct MyWorkflow {
    String field
}

workflow MyWorkflow {
    output {
        String result = "test"
    }
}
