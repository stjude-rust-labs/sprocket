version 1.3

# First definition with explicit type parameter
enum Status[String] {
    Active = "active",
    Pending = "pending"
}

# Second definition with inferred type parameter (should conflict)
enum Status {
    Active = "active",
    Pending = "pending"
}

workflow test {}
