## This is a test to ensure that input paths are mapped early so that
## intermediary expressions see the mapped values.

version 1.2

task test {
    input {
        Array[File] files
    }

    Array[String] args = squote(files)

    command <<<
        cat ~{sep(" ", args)}
    >>>

    output {
        String out = read_string(stdout())
    }
}
