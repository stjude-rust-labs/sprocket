version 1.2

struct Qux {
    Int num
}

workflow partial {
    Qux qux = Qux {
        num: 1,

    }

    output {
        Int out = qux.n
    }
}
