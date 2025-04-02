## This is a test of localizing remote files for task execution.

version 1.1

task t {
    # This path should not be localized or translated to a guest path
    File relative_path = "relative.txt"

    input {
        File one
        File two
        File local
    }

    command <<<
        set -euo pipefail
        cat '~{local}' > ~{relative_path}
        cat '~{one}' > one
        cat '~{two}' > two
    >>>

    output {
        File one_out = "one"
        File two_out = "two"
        File relative_out = relative_path
    }
}

workflow test {
    input {
        File one
        File two
        File local
    }

    call t { input: one, two, local }

    output {
        Object o1 = read_json(t.one_out)
        Object o2 = read_json(t.two_out)
        String s = read_string(t.relative_out)
    }
}
