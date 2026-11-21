version 1.4

task test {
    command <<<
        >&2 echo this task is going to fail!
        exit 1
    >>>
}
