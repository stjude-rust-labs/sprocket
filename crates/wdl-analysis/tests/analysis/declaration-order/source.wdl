version 1.2

task greet {
    String greeting = "Hello"

    command <<<
        echo "~{greeting}, ~{name}!"
    >>>

    String name = "World"
}