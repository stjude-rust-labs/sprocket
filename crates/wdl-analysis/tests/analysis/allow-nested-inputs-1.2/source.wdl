#@ except: UnusedInput, UnusedCall
## This is a test of allowed nested inputs in 1.2.
## This should be accepted without diagnostics.

version 1.2

task my_task {
    input {
        # Required
        String required
        # Optional
        String? optional
        # Defaulted
        String defaulted = "default"
    }

    command <<<>>>
}

workflow my_workflow {
    hints {
        allow_nested_inputs: true
    }

    # Missing required input
    call my_task

    # OK
    call my_task as my_task2 { input: required = "required" }

    # OK
    call my_task as my_task3 { input: required = "required", optional = "optional", defaulted = "defaulted" }
}
