#@ except: UnusedInput,UnusedDeclaration
## This is a test of conflicting declaration names.

version 1.1

task t {
    input {
        Int x
        Int y = 0
        String b
    }

    output {
        String x = "x"
        String y = "y"
        String a = "a"
    }

    Int z = x
    Int x = y

    command <<<>>>
}

workflow w {
    input {
        Int x
        Int y = 0
        String b
    }

    output {
        String x = "x"
        String y = "y"
        String a = "a"
    }

    Int z = x
    Int x = y

    if (true) {
        Int x2 = 0
        String really_ok = "ok"

        if (false) {
            Int b = 0
            Int x2 = 0
            String really_really_ok = "ok"
        }
    }

    scatter (x in [1, 2, 3]) {
        Int z = x
        String ok = "ok"

        scatter (nested in [1, 2, 3]) {
            scatter (baz in [1, 2, 3]) {
                Int nested = 0
            }
        }

        # This is ok as `baz` was a scatter variable no longer in scope
        Int baz = 0
        # However, this is a duplicate of `nested` within the scatter statement
        Int nested = 0
    }
}
