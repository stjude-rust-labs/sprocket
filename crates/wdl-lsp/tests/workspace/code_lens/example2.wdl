version 1.4

task greet {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
}