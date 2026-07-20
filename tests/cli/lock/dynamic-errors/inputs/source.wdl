version 1.2

task dynamic {
    input {
        String version
    }

    command <<<
        echo "dynamic"
    >>>

    requirements {
        container: "ubuntu:~{version}"
    }
}
