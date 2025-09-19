# This is a test of command sections.

version 1.1

task heredoc {
    input {
        String name = "world"
    }

    command <<<
        set -e
        printf "hello, ~{name}\\n"! >> output.txt
        printf "${ENV_VAR}" > env.txt # not interpolated
    >>>
}

task brace {
    input {
        String name = "world"
    }

    command {
        set -e
        printf "hello, ~{name}\\n"! >> output.txt
        printf "${ENV_VAR}" > env.txt # interpolated
    }
}
