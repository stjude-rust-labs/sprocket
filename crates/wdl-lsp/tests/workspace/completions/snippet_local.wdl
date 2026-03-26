version 1.3

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
