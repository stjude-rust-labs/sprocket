#@ except: UnusedDeclaration
version 1.3

# Valid enum with explicit type parameter
enum Status[String] {
    Active = "active",
    Pending = "pending",
    Complete = "complete"
}

# Valid enum with inferred type parameter
enum Priority {
    Low = 1,
    Medium = 2,
    High = 3
}

# Valid enum without values
enum Color {
    Red,
    Green,
    Blue
}

# Valid empty enum
enum Empty {}

workflow test {
    Status s = Status.Active
    Priority p = Priority.High
}
