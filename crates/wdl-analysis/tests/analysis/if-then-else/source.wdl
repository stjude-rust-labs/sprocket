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
        String my_variable = "foo"
        String my_greeting = "Hello"
        call sayColor {
            color = "red"
        }
    } else if (useGreen) {
        String my_variable = "bar"
        String my_greeting = "Hi"
        call sayColor {
            color = "green"
        }
    } else if (useBlue) {
        String my_variable = "baz"
        call sayColor {
            color = "blue"
        }
    } else {
        String my_variable = "quux"
        String my_greeting = "Salutations"
        call sayColor {
            color = "unknown"
        }
    }

    output {
        String variable = my_variable 
        String? greeting = my_greeting
    }
}