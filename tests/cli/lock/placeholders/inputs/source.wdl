version 1.4

task placeholders {
    input {
        String python_tag
    }

    command <<<>>>

    requirements {
        container: "python:~{python_tag}"
    }
}
