#@ except: UnusedDeclaration
## This is a test of type mismatches in comparison expressions.

version 1.1

task test {
    Int a = 1
    Int b = 2
    String c = "3"
    File d = "4"
    Float e = 1.0
    Boolean f = true

    Boolean e1 = a == b
    Boolean e2 = a == f
    Boolean e3 = f == a
    Boolean e4 = c == c
    Boolean e5 = d == d
    Boolean e6 = e == e
    Boolean e7 = a == e
    Boolean e8 = e == a

    Boolean ne1 = a != b
    Boolean ne2 = a != f
    Boolean ne3 = f != a
    Boolean ne4 = c != c
    Boolean ne5 = d != d
    Boolean ne6 = e != e
    Boolean ne7 = a != e
    Boolean ne8 = e != a

    Boolean l1 = a < b
    Boolean l2 = a < f
    Boolean l3 = f < a
    Boolean l4 = c < c
    Boolean l5 = d < d
    Boolean l6 = e < e
    Boolean l7 = a < e
    Boolean l8 = e < a

    Boolean le1 = a <= b
    Boolean le2 = a <= f
    Boolean le3 = f <= a
    Boolean le4 = c <= c
    Boolean le5 = d <= d
    Boolean le6 = e <= e
    Boolean le7 = a <= e
    Boolean le8 = e <= a

    Boolean g1 = a > b
    Boolean g2 = a > f
    Boolean g3 = f > a
    Boolean g4 = c > c
    Boolean g5 = d > d
    Boolean g6 = e > e
    Boolean g7 = a > e
    Boolean g8 = e > a

    Boolean ge1 = a >= b
    Boolean ge2 = a >= f
    Boolean ge3 = f >= a
    Boolean ge4 = c >= c
    Boolean ge5 = d >= d
    Boolean ge6 = e >= e
    Boolean ge7 = a >= e
    Boolean ge8 = e >= a

    command <<<>>>
}
