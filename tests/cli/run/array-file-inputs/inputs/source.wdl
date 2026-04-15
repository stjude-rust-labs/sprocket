version 1.3

task read_files {
    input {
        Array[File] files
    }

    command <<<
        for f in ~{sep(" ", files)}; do
            cat "$f"
        done
    >>>

    output {
        String result = read_string(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}
