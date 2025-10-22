## This is a test of conditional clauses with the same name in multiple clauses conflicting with parent scope

version 1.3

workflow test {
    Int a = 1

    if (true) {
        String a = "hello"
    } else if (false) {
        String a = "world"
    } else {
        String a = "goodbye"
    }

    output {
        Int out = a
    }
}
