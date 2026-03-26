version 1.3

task test {
    command <<<
        echo this task forgot to write to foo.txt!
    >>>

    output {
        File foo = "foo.txt"
    }
}
