version 1.2

task local {
    input {
        String name
    }

    command <<<
        echo 'hello, ~{name}'
    >>>
}

workflow test {
    call
}
