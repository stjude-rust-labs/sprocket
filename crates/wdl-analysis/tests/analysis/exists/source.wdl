version 1.4

enum Name {
    Labels,
}

task exists_task {
    input {
        File file
        Directory directory
        String path
    }

    Boolean input_exists = exists(file)

    command <<<
        printf "first\nsecond\n" > labels.txt
        mkdir results
    >>>

    output {
        Boolean input_file_exists = input_exists
        Boolean file_exists = exists("labels.txt")
        Boolean directory_exists = exists("results")
        Boolean missing_exists = exists("missing.txt")
        Boolean directory_input_exists = exists(directory)
        Boolean string_input_exists = exists(path)
        Boolean enum_exists = exists(Name.Labels)
        Array[String] labels = if exists("labels.txt") then read_lines("labels.txt") else []
    }
}

workflow test_exists {
    input {
        File file
        Directory directory
    }

    call exists_task {
        file,
        directory,
        path = "data.txt",
    }

    output {
        Boolean file_exists = exists(file)
        Boolean directory_exists = exists(directory)
        Boolean url_exists = exists("https://example.com/data.txt")
        Boolean task_file_exists = exists_task.file_exists
    }
}
