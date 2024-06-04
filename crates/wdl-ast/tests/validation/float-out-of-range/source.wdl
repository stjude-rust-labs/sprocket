# This is a test for out of range floats.

version 1.1

workflow test {
    Float a = 0.
    Float b = 0.0
    Float c = 1234.1234
    Float d = 123e123
    Float e = 0.1234
    Float f = 10.
    Float g = .2
    Float h = 1234.1234e1234
}
