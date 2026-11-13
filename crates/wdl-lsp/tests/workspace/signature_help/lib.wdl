version 1.3

## A task imported from a library.
task lib_task {
    input {
        ## The input file
        File file
        ## An optional flag
        Boolean flag = false
    }

    command <<<
        echo ~{file}
    >>>

    output {
        String out = stdout()
    }
}
