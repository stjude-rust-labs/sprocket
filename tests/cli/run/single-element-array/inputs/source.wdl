version 1.4

task echo_item {
    input {
        Array[String] items
    }

    command <<<
        echo ~{sep(" ", items)}
    >>>

    output {
        String result = read_string(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}
