version 1.3

task echo_items {
    input {
        Array[String] items
    }

    command <<<
        for item in ~{sep(" ", items)}; do
            echo "$item"
        done
    >>>

    output {
        Array[String] result = read_lines(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}
