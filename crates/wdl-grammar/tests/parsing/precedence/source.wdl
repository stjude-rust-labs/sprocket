# This is a test of operator precedence

version 1.1

task test {
    Boolean a = true || false && 1 == 0 != 1 < 0 <= 1 > 0 >= 1 + 2 - 3 * 4 / 5 % 6
    Int b = (1 + 2) - (3 * 4) / (5 % 6)
    Boolean c = 1 + 2 - 3 * 4 / 5 % 6 >= 0 > 1 <= 0 < 1 != 0 == 1 && false || true
}
