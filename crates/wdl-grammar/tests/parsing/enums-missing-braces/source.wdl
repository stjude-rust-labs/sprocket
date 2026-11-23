## Test enum with missing braces

version 1.4

# Missing braces - should error
enum Color
    Red,
    Green,
    Blue

workflow test {
    output {
        String result = "done"
    }
}
