version 1.4

task print_pairs {
    input {
        Array[Array[String]] pairs
    }

    command <<<
        greetings=(~{sep(" ", pairs[0])})
        targets=(~{sep(" ", pairs[1])})
        for i in "${!greetings[@]}"; do
            echo "${greetings[$i]} ${targets[$i]}"
        done
    >>>

    output {
        Array[String] lines = read_lines(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}
