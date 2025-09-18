## This is a test of having an unclean input path.
## The input path should still translate correctly.

version 1.2

task test {
    input {
        File file
    }

    command <<<
        cat '~{file}'
    >>>

    output {
        String message = read_string(stdout())
    }
}
