version 1.4

task say_hello {
    input {
        String one
        String two
        String three
    }

    command <<<
        echo "Hello, ~{one}, ~{two}, and ~{three}!"
    >>>
}
