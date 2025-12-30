## Test enum with malformed type parameter

version 1.3

# Missing closing bracket in type parameter
enum Status[String {
    Pending,
    Complete
}

# Missing type in brackets
enum Priority[] {
    Low,
    High
}

workflow test {
    output {
        String result = "done"
    }
}
