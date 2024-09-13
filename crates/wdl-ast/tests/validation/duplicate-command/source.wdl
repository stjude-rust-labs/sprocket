# This is a test of too many command sections in a task.

version 1.1

task test {
    command <<<>>>
    command <<<>>>
}

# This duplicate task should be ignored.
task test {
    command <<<>>>
    command <<<>>>
}
