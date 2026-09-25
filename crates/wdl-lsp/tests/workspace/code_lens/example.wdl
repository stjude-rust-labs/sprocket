version 1.1

task say_hello {
    command {
        echo "Hello, world!"
    }
}

workflow wrap_say_hello {
    call say_hello
}
