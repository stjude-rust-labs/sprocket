version 1.3

task placeholders {
    input {
        String python_tag
    }

    command <<<>>>

    requirements {
        container: "python:~{python_tag}"
    }
}
