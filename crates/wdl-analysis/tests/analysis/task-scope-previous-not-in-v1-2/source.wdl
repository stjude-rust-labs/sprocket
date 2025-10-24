## Test that `task.previous` is not available in WDL 1.2.

version 1.2

task test_previous_not_in_v12 {
  command <<<
    # `task.previous` should not be available in WDL 1.2, even in
    # post-evaluation contexts.
    echo ~{task.previous.memory}
  >>>
}
