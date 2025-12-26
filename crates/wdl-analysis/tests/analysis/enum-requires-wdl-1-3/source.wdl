#@ except: UnusedDeclaration

version 1.2

enum Status {
    Active,
    Pending,
    Complete
}

workflow test {
    Status s = Status.Active
}
