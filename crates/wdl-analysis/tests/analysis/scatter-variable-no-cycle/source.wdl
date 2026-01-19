## This is a test to ensure scatter variable dependency edges are added correctly
## It should not generate any diagnostics
## See: https://github.com/stjude-rust-labs/sprocket/issues/508
version 1.3

task bar {
    input {
        Int foo
    }

    command <<<
        echo ~{foo}
    >>>

    output {
        Int out = foo
    }
}

workflow scopes {
    scatter (foo in [1, 2, 3, 4, 5]) {
        # The reference to `foo` here should be to the scatter variable
        # and not to the private declaration `foo` below
        call bar {
            foo
        }
    }

    scatter (not_foo in bar.out) {
        Int foo = not_foo
        call bar as bar2 {
            foo
        }
    }
}
