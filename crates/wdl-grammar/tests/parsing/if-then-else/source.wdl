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

    if (useRed) {
        call sayColor {
            color = "red"
        }
    } else if (useGreen) {
        call sayColor {
            color = "green"
        }
    } else if (useBlue) {
        call sayColor {
            color = "blue"
        }
    } else {
        call sayColor {
            color = "unknown"
        }
    }
}
