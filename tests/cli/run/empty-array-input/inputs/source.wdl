version 1.3

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
