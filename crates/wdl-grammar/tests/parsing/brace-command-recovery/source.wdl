# This is a test of a recovery in a brace command.

version 1.1

task test {
    command {
        before ${!} after
    }

    runtime {
        foo: "bar"
    }
}
