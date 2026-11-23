version 1.4

enum Status {
    Active,
    Inactive,
    Pending
}

workflow test {
    Status a = Status.InvalidChoice

    output {
        Status b = a
    }
}
