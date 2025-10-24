## Test that `task.max_retries` is not available in WDL 1.2.

version 1.2

task test_max_retries_not_in_v12 {
  command <<<
    # `task.max_retries` should not be available in WDL 1.2, even in
    # post-evaluation contexts.
    echo ~{task.max_retries}
  >>>
}
