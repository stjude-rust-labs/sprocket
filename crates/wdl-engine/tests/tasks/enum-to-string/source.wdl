version 1.3

enum Color {
    Red,
    Green,
    Blue
}

task enum_to_string {
    input {
        Color color = Color.Red
    }

    command <<<
        echo "~{color}"
    >>>

    output {
        String color_name = "~{color}"
        String from_stdout = read_string(stdout())
    }
}
