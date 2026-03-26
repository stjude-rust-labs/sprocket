## This is a test of symlinking to an input file from within a task.
## In this case, the file is a remote URL, so the symlink should be replaced with a path from the download cache.
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
