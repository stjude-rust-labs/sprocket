## This is a test of the input validation reference restrictions.

version 1.4

task t {
    input {
        Int a
        # Default references a private declaration
        Int b = private
        # Transitively references a private declaration
        Int c = b + 1
        # Only references other inputs
        Int d = a + 1

        validation {
            private > 0: "cannot reference a private declaration",
            b > 0: "cannot reference an input that depends on a private declaration",
            c > 0: "cannot reference an input that transitively depends on a private declaration",
            d > 0: "ok (~{a})",
            unknown > 0: "unknown name",
            out > 0: "outputs are not in scope",
        }
    }

    Int private = 1

    command <<<>>>

    output {
        Int out = a + b + c + d + private
    }
}

workflow w {
    input {
        Int a
        # Default references a call output
        Int b = t.out
        # Transitively references a call output
        Int c = b
        # Default references a private declaration
        String s = private

        validation {
            private != "": "cannot reference a private declaration",
            t.out > 0: "cannot reference a call output",
            b > 0: "cannot reference an input that depends on a call output",
            c > 0: "cannot reference an input that transitively depends on a call output",
            s != "": "cannot reference an input that depends on a private declaration",
            a > 0: "ok",
        }
    }

    String private = "foo"

    call t { a }

    output {
        Int out = a + b + c
        String str = s
    }
}
