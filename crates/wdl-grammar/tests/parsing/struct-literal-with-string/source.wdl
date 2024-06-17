## This is a test of having a struct literal with string member names.
## This should result in errors as only identifiers are expected.

version 1.1

workflow test {
    # This is not a legal struct literal
    Runtime standard_runtime = Runtime {"gatk_docker": gatk_docker,
                                        "cpu": small_task_cpu,
                                        "machine_mem": small_task_mem * 1024,
                                        "command_mem": (small_task_mem * 1024) - 512}

    # This is also here to check that we correctly handle this _not_ as a struct literal
    if foo {
        "this is not legal either"
    }
}
