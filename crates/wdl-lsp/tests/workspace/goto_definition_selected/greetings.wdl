version 1.4

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>

    output {
        String stdout = read_string(stdout())
    }
}

workflow do_greetings {
    input {
        String name
    }

    call say_hello {
        name
    }

    output {
        String stdout = say_hello.stdout
    }
}
