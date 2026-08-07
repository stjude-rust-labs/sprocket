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

task baz {
    meta {
        foo: {
            bar: {}
}

task qux {
    hints {
        inputs: {
            foo: {
                bar: {
                    baz: {
                        qux: {
                            quux: {
}