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

## A workflow that orchestrates greetings.
## Useful for testing workflow doc comments.
workflow doc_workflow {
    input {
        ## The recipient of the greeting
        String recipient
    }

    call doc_only { input: name = recipient }

    output {
        String result = doc_only.out
    }
}

## A person with a name and age.
## Used to test struct doc comments.
struct DocPerson {
    ## The person's full name
    String name
    ## The person's age in years
    Int age
}

## A status indicator enum.
enum DocStatus {
    Active,
    Inactive,
}

## First paragraph of doc.
##
## Second paragraph after blank line.
task blank_line_doc {
    command <<<>>>
}
