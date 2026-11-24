version 1.3

task say_hello {
    input {
        String container
    }

    meta {
        container: "My key should be ignored"
    }

    parameter_meta {
        container: {
            description: "Hopefully my key gets updated too...",
            container: "But not me!"
        }
    }

    Object obj = object {
        container: container
    }

    command <<<
        echo "Hello, World! Reporting from `~{task.container}`"
    >>>

    requirements {
        container: container
    }

    hints {
        container: container
    }
}
