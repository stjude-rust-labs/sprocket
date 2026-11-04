# This is a test of the postfix unwrap operator

version 1.4

workflow test {
    Int a = x!
    Int b = -x!
    Boolean c = !y!
    Int d = a + b!
    Int e = a ** b!
    Int f = person.income!.amount
    Int g = xs![0]!
    Int h = f(x)!
    Int i = x!!
    Boolean j = x! == y
    Boolean k = x != y
    Boolean l = (x!) == y
    String m = "~{name! + ' '}"
    Int n = x !
    Boolean o = x!=y
}
