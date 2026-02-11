version 1.3

## An RGB24 color enum
##
## Each variant is represented as a 24-bit hexadecimal RGB string with exactly one non-zero channel.
enum Color[String] {
    ## Pure red
    Red = "#FF0000",
    ## Pure green
    ##
    ## Some really long description about green.
    ##
    ## Lorem ipsum dolor sit amet consectetur adipiscing elit.
    ## Quisque faucibus ex sapien vitae pellentesque sem placerat.
    ## In id cursus mi pretium tellus duis convallis.
    ## Tempus leo eu aenean sed diam urna tempor.
    ##
    ## Lorem ipsum dolor sit amet consectetur adipiscing elit.
    ## Quisque faucibus ex sapien vitae pellentesque sem placerat.
    Green = "#00FF00",
    Blue = "#0000FF" # No description
}

workflow favorite_color {
    meta {
        description: "Returns the caller's favorite color."
        outputs: {
            result: "The caller's favorite color."
        }
    }

    input {
        Color favorite_color
    }

    output {
        Color result = favorite_color
    }
}

task is_red {
    meta {
        description: "Determines if a color is red."
        outputs: {
            result: "Whether the input is red."
        }
    }

    input {
        Color color
    }

    command { }

    output {
        Boolean result = color == Color.Red
    }
}

task is_green {
    meta {
        description: "Determines if a color is green."
        outputs: {
            result: "Whether the input is green."
        }
    }

    input {
        Color color
    }

    command { }

    output {
        Boolean result = color == Color.Green
    }
}

task is_blue {
    meta {
        description: "Determines if a color is blue."
        outputs: {
            result: "Whether the input is blue."
        }
    }

    input {
        Color color
    }

    command { }

    output {
        Boolean result = color == Color.Blue
    }
}