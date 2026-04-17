version 1.3

task count_items {
    input {
        String name
        Array[String] items
    }

    command <<<
        echo ~{name}: ~{length(items)}
    >>>

    output {
        String result = read_string(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}
