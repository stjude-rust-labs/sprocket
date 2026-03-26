version 1.3

task test {
    command <<<
        >&2 echo this task is going to fail!
        exit 1
    >>>
}
