# This is a test for the `0o` and `0b` literal integer prefixes, which require
# WDL 1.1 or later.

version 1.0

workflow test {
    meta {
        a: 0o755
        b: -0o755
        c: [0b1, 0B10]
    }

    parameter_meta {
        x: { nested: -0O17 }
    }

    input {
        Int x = 0
    }

    Int a = 0o755
    Int b = -0O755
    Int c = 0b1010
    Int d = 0B1
    Int e = 0x1F
}
