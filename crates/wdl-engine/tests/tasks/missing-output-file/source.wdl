version 1.2

task test {
    command <<<
        echo this task forgot to write to foo.txt!
    >>>

    output {
        File foo = "foo.txt"
    }
}
