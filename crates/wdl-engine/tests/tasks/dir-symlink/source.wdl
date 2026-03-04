## This is a test of symlinking to a file within a directory input from within a task.
version 1.3

task test {
    input {
        Directory dir
    }

    command <<<
        ln -s '~{dir}/input.txt' foo
    >>>

    output {
        File out = "foo"
    }
}
