version 1.3

# Duplicate sections are a validation error, but formatting must still retain
# every section that was written. See issue #852.
struct MyStruct {
    meta {
        description: "first meta"
    }

    meta {
        description: "second meta"
    }

    parameter_meta {
        member: "first parameter_meta"
    }

    parameter_meta {
        member: "second parameter_meta"
    }

    String member
}

task a_task_with_multiple_runtimes {
    #@ except: MetaDescription
    meta {
    }

    meta {
        description: "second meta"
    }

    command <<<
    >>>

    output {
    }

    output {
        String out = "second output"
    }

    # The lints should only appear for this runtime.
    runtime {
        foo: "bar"  # these items should be processed and flagged.
        baz: "quux"
    }

    # This should be reported as a validation error with no
    # lint warnings.
    runtime {
        foo: "bar"  # these items should not be processed and flagged.
        baz: "quux"
    }

    # A task may only have one of `requirements` and `runtime`; when it has
    # both, they are written in the order they appeared.
    requirements {
        container: "ubuntu:latest"
    }

    hints {
        max_cpu: 1
    }

    hints {
        max_memory: "1 GiB"
    }
}

workflow a_workflow_with_duplicate_sections {
    meta {
        description: "first meta"
    }

    meta {
        description: "second meta"
    }

    input {
        String first
    }

    input {
        String second
    }

    call a_task_with_multiple_runtimes

    output {
    }

    output {
        String out = "second output"
    }

    hints {
        allow_nested_inputs: true
    }

    hints {
        allow_nested_inputs: false
    }
}
