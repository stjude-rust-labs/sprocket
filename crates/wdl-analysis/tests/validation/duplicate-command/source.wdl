# This is a test of too many command sections in a task.

version 1.1

task test {
    command <<<>>>
    command <<<>>>
}

# A duplicate task should trigger a single error and then be ignored.
task test {
    command <<<>>>
    command <<<>>>
}
