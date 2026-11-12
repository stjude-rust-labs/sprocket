# This is a test for legacy octal literal integers, which are not permitted in
# WDL 1.1 or later.

version 1.1

workflow test {
    meta {
        a: 0755
        b: -0755
        c: [00, 01]
    }

    parameter_meta {
        x: { nested: -0777 }
    }

    input {
        Int x = 0
    }

    Int a = 0755
    Int b = -0755
    Int c = 0000
    Int d = 01777777777777777777777777
    Int e = 0 + 0755
}
