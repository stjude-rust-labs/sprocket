## This is a test of missing braces

version 1.1

task foo {
    input {
        String x = "nope"
    
    meta {
        foo: "bar"
    }
}

task bar {
    input {
    output {
}
