version 1.3

task do_say_hello {
    command <<<
        echo "Hello, world!"
    >>>
}

workflow say_hello {
    call do_say_hello
}