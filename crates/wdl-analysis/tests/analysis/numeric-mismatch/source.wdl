## This is a test of type mismatches in numeric expressions.

#@ except: UnusedDeclaration

version 1.3

task test {
    Int a = 1
    Int b = 2
    String c = "3"
    File d = "4"
    Float e = 1.0
    Int? f = 1
    String? g = "3"
    File? h = "4"
    Float? i = 1.0
    Boolean j = true

    Int a1 = a + b      # OK
    String a2 = a + c   # OK
    String a3 = c + a   # OK
    String a4 = c + c   # OK
    File a5 = d + d     # NOT OK
    Float a6 = e + e    # OK
    Float a7 = a + e    # OK
    Float a8 = e + a    # OK
    String a9 = c + f   # NOT OK
    String a10 = c + g  # NOT OK
    String a11 = c + h  # NOT OK
    String a12 = c + i  # NOT OK

    # Check for string concatenation in interpolation context that allows optional types
    # None of these should error
    String i1 = "~{f + c}"
    String i2 = "~{c + f}"
    String i3 = "~{g + c}"
    String i4 = "~{c + g}"
    String i5 = "~{h + c}"
    String i6 = "~{c + h}"
    String i7 = "~{i + c}"
    String i8 = "~{c + i}"

    Int s1 = a - b      # OK
    Boolean s2 = a - j  # NOT OK
    Boolean s3 = j - a  # NOT OK
    Boolean s4 = j - j  # NOT OK
    File s5 = d - d     # NOT OK
    Float s6 = e - e    # OK
    Float s7 = a - e    # OK
    Float s8 = e - a    # OK

    Int m1 = a * b      # OK
    Boolean m2 = a * j  # NOT OK
    Boolean m3 = j * a  # NOT OK
    Boolean m4 = j * j  # NOT OK
    File m5 = d * d     # NOT OK
    Float m6 = e * e    # OK
    Float m7 = a * e    # OK
    Float m8 = e * a    # OK

    Int d1 = a / b      # OK
    Boolean d2 = a / j  # NOT OK
    Boolean d3 = j / a  # NOT OK
    Boolean d4 = j / j  # NOT OK
    File d5 = d / d     # NOT OK
    Float d6 = e / e    # OK
    Float d7 = a / e    # OK
    Float d8 = e / a    # OK

    Int mod1 = a % b      # OK
    Boolean mod2 = a % j  # NOT OK
    Boolean mod3 = j % a  # NOT OK
    Boolean mod4 = j % j  # NOT OK
    File mod5 = d % d     # NOT OK
    Float mod6 = e % e    # OK
    Float mod7 = a % e    # OK
    Float mop8 = e % a    # OK

    Int e1 = a ** b      # OK
    Boolean e2 = a ** j  # NOT OK
    Boolean e3 = j ** a  # NOT OK
    Boolean e4 = j ** j  # NOT OK
    File e5 = d ** d     # NOT OK
    Float e6 = e ** e    # OK
    Float e7 = a ** e    # OK
    Float e8 = e ** a    # OK

    command <<<>>>
}
