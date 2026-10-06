version 1.3

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
