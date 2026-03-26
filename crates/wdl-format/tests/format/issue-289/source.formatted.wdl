version 1.3

struct Person {
    String name
    Int age
}

workflow main {
    Array[Person] person = [
        Person {
            name: "Jane Doe",
            age: 30,
        },
        Person {
            name: "Jane Doe",
            age: 30,
        },
    ]

    output {
        File result = write_tsv(person)
    }
}
