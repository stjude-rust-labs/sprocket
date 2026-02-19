# https://github.com/stjude-rust-labs/sprocket/issues/292
version 1.0

workflow test {
    input {
        String foo
        String bar
        String baz
    }

    call something { input:
        foo, # Bad
        bar, # Bad
        baz = baz, # Good
    }
}

task something {
    input {
        String foo
        String bar
        String baz
    }

    command <<<>>>
}