version 1.2

struct MyWorkflow {
    String field
}

workflow MyWorkflow {
    output {
        String result = "test"
    }
}
