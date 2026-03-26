version 1.3

enum Status {
    Active,
    Inactive,
    Pending
}

workflow test {
    Status s = Status.InvalidVariant
}
