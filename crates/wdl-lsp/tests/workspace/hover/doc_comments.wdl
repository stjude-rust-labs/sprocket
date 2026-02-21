version 1.3

## Greets someone by name.
## Used for hover doc tests.
task doc_only {
    input {
        ## The person's name to greet
        String name
    }

    command <<<
        echo "hello ~{name}"
    >>>

    output {
        String out = read_string(stdout())
    }
}

## This doc comment should win over meta.
task doc_and_meta {
    meta {
        description: "This meta description should NOT appear"
    }

    input {
        String name
    }

    command <<<
        echo "hello ~{name}"
    >>>

    output {
        String out = read_string(stdout())
    }
}

task meta_only {
    meta {
        description: "A simple greeting task"
    }

    input {
        String name
    }

    command <<<
        echo "hello ~{name}"
    >>>

    output {
        String out = read_string(stdout())
    }
}
