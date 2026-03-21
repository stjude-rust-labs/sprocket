version 1.3

task foo {
    input {
        String name
    }

    command <<<
        echo "~{name}"
    >>>

    requirements {
        container: "ubuntu:latest"
    }
}

task bar {
    input {
        String name
    }

    command <<<
        echo "~{name}"
    >>>

    requirements {
        container: "ubuntu:latest"
    }
}
