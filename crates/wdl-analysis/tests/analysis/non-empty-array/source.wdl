#@ except: UnusedDeclaration, UnusedInput, UnusedCall
## This is a test of assigning an empty array to a non-empty array declaration.

version 1.1

task t {
    input {
        Array[Int]+ x
    }

    command <<<>>>
}

workflow test {
    # This is an error (cannot assign empty array)
    Array[Int]+ x = []

    # This is OK
    Array[Int] y = []

    # This is OK (checked at runtime)
    Array[Int]+ z = y

    # These are OK
    call t { input: x }
    call t as t2 { input: x = x }
    call t as t3 { input: x = y }

    # This is not OK because it's an empty array literal
    call t as t4 { input: x = ((([]))) }
}
