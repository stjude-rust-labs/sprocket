#@ except: DescriptionMissing

version 1.0

task a_task_with_no_keys {
    meta {}

    command <<<>>>

    output {}

    runtime {}  # Two missing keys: "docker" and "memory"
}

task a_task_with_excepted_runtime {
    meta {}

    command <<<>>>

    output {}

    #@ except: RuntimeSectionKeys
    runtime {}  # Errors should be ignored
}

task a_task_with_only_the_docker_key {
    meta {}

    command <<<>>>

    output {}

    runtime {
        docker: "foo"
    }
}

task a_task_with_only_the_memory_key {
    meta {}

    command <<<>>>

    output {}

    runtime {
        memory: "foo"
    }
}

task a_task_with_both {
    meta {}

    command <<<>>>

    output {}

    runtime {
        docker: "foo"
        memory: "bar"
    }
}

task a_task_with_extra_keys_but_no_errors {
    meta {}

    command <<<>>>

    output {}

    runtime {
        docker: "foo"
        memory: "bar"
        baz: "quux"  # this should not throw an error
    }
}
