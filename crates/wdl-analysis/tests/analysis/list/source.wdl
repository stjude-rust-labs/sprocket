version 1.4

task list_task {
    input {
        Directory directory
    }

    Pair[Array[File], Array[Directory]] before_command = list(directory)

    command <<<>>>

    output {
        Array[File] files = before_command.left
        Array[Directory] directories = before_command.right
        Pair[Array[File], Array[Directory]] recursive = list(directory, true)
        Pair[Array[File], Array[Directory]] without_symlinks = list(directory, true, false)
        Pair[Array[File], Array[Directory]] filtered = list(directory, true, false, "*.txt")
    }
}

workflow test_list {
    input {
        Directory directory
    }

    call list_task {
        input:
            directory
    }

    output {
        Pair[Array[File], Array[Directory]] entries = list(directory)
        Pair[Array[File], Array[Directory]] recursive = list(directory, true)
        Pair[Array[File], Array[Directory]] without_symlinks = list(directory, true, false)
        Pair[Array[File], Array[Directory]] filtered = list(directory, true, false, "*.txt")
        Pair[Array[File], Array[Directory]] string_path = list("data")
        Array[File] files = filtered.left
        Array[Directory] directories = filtered.right
        Array[File] task_files = list_task.files
        Array[Directory] task_directories = list_task.directories
    }
}
