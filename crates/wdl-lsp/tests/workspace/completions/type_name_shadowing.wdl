version 1.3

enum Status {
    Active,
    Inactive
}

enum Salutation {
    Hi,
    Hello,
    Hej 
}

struct Point {
    Int x
    Int y
}

workflow example {
    input {
        Point Status = Point { x: 1, y: 2 }
    }

    Int x = 1
    Int y = 
}
