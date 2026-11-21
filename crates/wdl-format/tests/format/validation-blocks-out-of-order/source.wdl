## Validation blocks that are not the last item in a section are invalid, but
## the formatter must preserve their position and never sort across them.

version 1.4

task t {
    input {
        String d
        String c
        validation { c != "": "c" }
        String b
        String a
        validation { a != "": "a" }
    }
    command <<<>>>
    output {
        String x = a
        validation { x != "": "x" }
        String y = b
    }
}
