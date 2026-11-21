## Tests that struct names may conflict with task names.
version 1.4

struct MyWorkflow {
    String field
}

workflow MyWorkflow {
    output {
        String result = "test"
    }
}
