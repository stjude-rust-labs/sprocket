# Test for lint tag argument precedence. `--tag` should take precedence over `--except`, so that
# patterns like `--exclude all --tag portability`, where we *only* enable `portability` lints, are possible.

version 1.3

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
}
