version 1.2

task test_runtime_info_task {
    # `task.return_code` is output-only, should fail
    command <<<
        echo ~{task.return_code}
    >>>

    output {
        # Should be fine
        Int return_code = task.return_code
    }
}