# This is a test for the accepted literal integer and float forms in WDL 1.1.

version 1.1

workflow test {
    meta {
        a: 0
        b: 1234
        c: 0x1F
        d: 0X1f
        e: 0o755
        f: 0O17
        g: 0b1010
        h: 0B1
        i: -0o755
        j: -0b1010
        k: [0o1, 0b1, -0x1]
        l: {
            nested: -0o1000000000000000000000,
        }
        m: 1e3
        n: 1.
        o: .5
    }

    parameter_meta {
        x: -0b1000000000000000000000000000000000000000000000000000000000000000
    }

    input {
        Int x = 0
    }

    Int a = 0
    Int b = 1234
    Int c = 0x1F
    Int d = 0X1f
    Int e = 0o755
    Int f = 0O17
    Int g = 0b1010
    Int h = 0B1
    Int i = -0o1000000000000000000000
    Int j = 0o777777777777777777777
    Int k = -0b1000000000000000000000000000000000000000000000000000000000000000
    Float l = 1e3
    Float m = 1E+3
    Float n = 1.
    Float o = 1.5e-3
    Float p = .5
    Float q = .5e1
    Float r = 00.5
}
