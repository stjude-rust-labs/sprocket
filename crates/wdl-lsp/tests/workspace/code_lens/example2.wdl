version 1.3

task greet {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
}