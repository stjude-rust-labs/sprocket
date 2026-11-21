version 1.4

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
