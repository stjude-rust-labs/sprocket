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
        Array[String] greetings = select_all([
            "Hello",
            "Hallo",
            "Hej",
            (
                if is_pirate
                then "Ahoy"
                else None
            ),
        ])
        String color = "green"
        Boolean is_pirate = false
    }

    scatter (greeting in greetings) {
        call say_hello {
            greeting,
            name,
        }
    }

    output {
        Array[String] messages = say_hello.message
    }
}