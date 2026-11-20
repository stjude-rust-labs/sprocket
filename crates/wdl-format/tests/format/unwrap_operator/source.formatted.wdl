version 1.4

workflow test {
    input {
        Int? x
        Int? y
        Array[Int?] xs = []
        String? name
    }

    Int a = x!
    Int b = -x!
    Int c = x! + y!
    Boolean d = x! == y!
    Boolean e = x! != y!
    Boolean f = !(x! > 1)
    Int g = xs[0]!
    Int h = select_first([x])
    Int i = (x)!
    String j = "~{name! + ' '}hello"
}
