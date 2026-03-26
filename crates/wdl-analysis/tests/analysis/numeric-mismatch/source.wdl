#@ except: UnusedDeclaration
## This is a test of type mismatches in numeric expressions.

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
    String s2 = a - c   # NOT OK
    String s3 = c - a   # NOT OK
    String s4 = c - c   # NOT OK
    File s5 = d - d     # NOT OK
    Float s6 = e - e    # OK
    Float s7 = a - e    # OK
    Float s8 = e - a    # OK

    Int m1 = a * b      # OK
    String m2 = a * c   # NOT OK
    String m3 = c * a   # NOT OK
    String m4 = c * c   # NOT OK
    File m5 = d * d     # NOT OK
    Float m6 = e * e    # OK
    Float m7 = a * e    # OK
    Float m8 = e * a    # OK

    Int d1 = a / b      # OK
    String d2 = a / c   # NOT OK
    String d3 = c / a   # NOT OK
    String d4 = c / c   # NOT OK
    File d5 = d / d     # NOT OK
    Float d6 = e / e    # OK
    Float d7 = a / e    # OK
    Float d8 = e / a    # OK

    Int mod1 = a % b      # OK
    String mod2 = a % c   # NOT OK
    String mod3 = c % a   # NOT OK
    String mod4 = c % c   # NOT OK
    File mod5 = d % d     # NOT OK
    Float mod6 = e % e    # OK
    Float mod7 = a % e    # OK
    Float mop8 = e % a    # OK

    Int e1 = a ** b      # OK
    String e2 = a ** c   # NOT OK
    String e3 = c ** a   # NOT OK
    String e4 = c ** c   # NOT OK
    File e5 = d ** d     # NOT OK
    Float e6 = e ** e    # OK
    Float e7 = a ** e    # OK
    Float e8 = e ** a    # OK
    
    command <<<>>>
}
