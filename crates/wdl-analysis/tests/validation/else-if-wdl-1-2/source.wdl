## This is a test of else if and else conditionals not being supported in WDL 1.2
version 1.2

workflow test {
    input {
        Boolean useRed = true
        Boolean useBlue = false
        Boolean useGreen = false
    }

    if (useRed) {
        String color = "red"
    } else if (useBlue) {
        String color = "blue"
    } else if (useGreen) {
        String color = "green"
    } else {
        String color = "unknown"
    }

    output {
        String out = color
    }
}
