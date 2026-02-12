## Test enum with invalid choice syntax

version 1.3

# Missing choice name before assignment
enum Priority[Int] {
    = 1,
    Low = 2
}

# Invalid expression syntax
enum Status {
    Pending =,
    Complete
}

workflow test {
    output {
        String result = "done"
    }
}
