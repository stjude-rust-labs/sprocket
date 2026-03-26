#@ except: UnusedDeclaration

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

enum HexColor {
  Red = "FF0000",
  Green = "00FF00",
  Blue = "0000FF"
}

workflow test {
    Status s = Status.Active
    Priority p = Priority.High

    String status_value = value(s)
    Int priority_value = value(p)

    String status_name = "~{s}"
    String priority_name = "~{p}"

    String hex_red = value(HexColor.Red)
}
