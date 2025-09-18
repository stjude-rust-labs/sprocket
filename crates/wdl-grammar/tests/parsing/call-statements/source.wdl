# This is a test of call statements.

version 1.1

workflow test {
    call no_params
    call with_params { input: a, b, c, d = 1 }
    call qualified.name
    call qualified.name { input: a = 1, b = 2, c = "3" }
    call aliased as x
    call aliased as x { input: }
    call f after x after y
    call f after x after y { input: a = [] }
    call f as x after x
    call f as x after x after y { input: name = "hello" }
}
