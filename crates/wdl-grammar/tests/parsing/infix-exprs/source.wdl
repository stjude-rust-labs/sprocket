# Test for infix expressions

version 1.1

task test {
    Boolean a = true || false
    Boolean b = a && true
    Boolean c = a == b
    Boolean d = c != false
    Boolean e = 1 < 2
    Boolean f = 2 <= 2
    Boolean g = 1 > 2
    Boolean h = 2 >= 2
    Integer i = 30 + 12
    Integer j = 30 - -12
    Integer k = 10 * 10
    Integer l = 10 / 10
    Integer m = 10 % 10
}
