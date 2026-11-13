# This is a test for legacy octal literal integers, which are permitted in WDL
# 1.0.

version 1.0

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
    Int d = 0x1F
    Int e = 0 + 0755
}
