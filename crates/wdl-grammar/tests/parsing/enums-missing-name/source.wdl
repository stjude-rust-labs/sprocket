## Test enum with missing name

version 1.4

# Missing enum name - should error
enum {
    First,
    Second
}

workflow test {
    output {
        String result = "done"
    }
}
