# This is a test of conditional workflow statements.

version 1.1

workflow test {
    if true {
        if false {
            scatter (x in y) {
                if true {
                    call z
                }
            }
        }

        # Ensure `x` is a name reference and not a struct literal
        if x {
            call y
        }

        call z { input: name = "world" }
        call z { input: name = "you" }
    }
}
