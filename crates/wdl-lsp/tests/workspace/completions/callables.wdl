version 1.4

task say_hello {
    command <<<
        echo "Hello, world!"
    >>>
}

workflow do_greetings {
    call say_hello as first
    call say_hello as second after f
    call s
}

workflow do_greetings_v2 {
    call 
}