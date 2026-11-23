version 1.4

task do_say_hello {
    command <<<
        echo "Hello, world!"
    >>>
}

workflow say_hello {
    call do_say_hello
}
