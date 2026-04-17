version 1.3

task sum_numbers {
    input {
        Array[Int] numbers
    }

    command <<<
        echo $(( ~{sep(" + ", numbers)} ))
    >>>

    output {
        Int total = read_int(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}
