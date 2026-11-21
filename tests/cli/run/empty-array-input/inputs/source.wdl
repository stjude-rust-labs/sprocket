version 1.4

task count_items {
    input {
        Array[String] items
    }

    command <<<
        echo ~{length(items)}
    >>>

    output {
        String count = read_string(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}
