version 1.2

task say_hello {
    input {
        String greeting
        String name
    }

    command <<<
        echo "~{greeting}, ~{name}!"
    >>>

    output {
        String message = read_string(stdout())
    }

    requirements {
        container: "ubuntu:latest"
    }
}

workflow main {
    input {
        String name
        Array[String] greetings = [
            "Hello",
            "Hallo",
            "Hej",
        ]
        String color = "green"
        Boolean is_pirate = false
    }

    scatter (greeting in greetings) {
        call say_hello {
            greeting,
            name,
        }
    }

    if (is_pirate) {
        call say_hello as hello_pirate {
            greeting = "Ahoy",
            name,
        }
    }

    output {
        Array[String] messages = flatten([
            say_hello.message,
            select_all([hello_pirate.message]),
        ])
    }
}
