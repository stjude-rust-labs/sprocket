## This is a test of missing both the meta and parameter_meta

version 1.0

workflow test {
    input {
        File input_file
    }
    call test_task { input:
        input_file = input_file
    }
    output {
        File output_file = test_task.output_file
    }
}

# This should not have diagnostics for <= 1.2
struct Test {
    String x
}
