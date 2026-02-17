## This is a test of symlinking to an input file from within a task.
version 1.3

task test {
    input {
        File file
    }

    command <<<
        ln -s '~{file}' foo
    >>>

    output {
        File out = "foo"
    }
}
