version 1.3

enum Color {
    Red,
    Green,
    Blue
}

task paint {
    input {
        Color color = Color.Red
    }

    command <<<
        echo "~{color}"
    >>>
}

workflow main {
    call paint {
        input:
            color = Color.Blue
    }
}
