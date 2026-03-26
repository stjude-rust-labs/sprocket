version 1.3

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
