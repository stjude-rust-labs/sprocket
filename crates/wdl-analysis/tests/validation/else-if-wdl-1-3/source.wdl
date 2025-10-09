## This is a test of else if and else conditionals being supported in WDL 1.3
version 1.3

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
