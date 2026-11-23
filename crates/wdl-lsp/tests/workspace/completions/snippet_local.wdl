version 1.4

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
