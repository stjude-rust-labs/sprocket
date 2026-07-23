version 1.3

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