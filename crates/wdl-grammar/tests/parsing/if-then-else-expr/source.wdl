version 1.2

task sayColor {
    input {
        String color
    }

    command <<<
        echo "Hello, ~{color} color!"
    >>>

    output {
        String out = read_string(stdout())
    }
}

workflow foo {
    Boolean useRed = false
    Boolean useGreen = false
    Boolean useBlue = false

    String color = if useRed then 
        "red"
    else if useGreen then
        "green"
    else if useBlue then
        "blue"
    else
        "unknown"

    call sayColor { color }
}
