## This is a test of enum parsing.

version 1.3

# Basic enum without type parameter
enum Color {
    Red,
    Green,
    Blue
}

# Enum with explicit `String` type parameter
enum Status[String] {
    Pending,
    Running,
    Complete
}

# Enum with explicit `Int` type parameter and values
enum Priority[Int] {
    Low = 1,
    Medium = 2,
    High = 3
}

# Empty enum
enum Empty {
}

# Single variant enum
enum Single {
    OnlyOne
}

# Enum with trailing comma
enum WithTrailingComma {
    First,
    Second,
    Third,
}

# Enum with mixed assignment patterns
enum Mixed[Int] {
    First = 1,
    Second,
    Third = 3
}

# Enum with complex expressions as values
enum Complex[String] {
    A = "hello",
    B = "world" + "!",
    C = if true then "yes" else "no"
}

workflow test {
    output {
        String result = "done"
        Mixed foo = Mixed.First
    }
}
