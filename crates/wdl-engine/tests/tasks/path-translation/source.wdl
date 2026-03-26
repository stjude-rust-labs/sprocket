## This is a test of properly translating paths in various expressions.

version 1.3

task test {
    input {
        Array[File] files
    }

    command <<<
        set -euo pipefail

        # Test for the `sep` option
        echo '~{if task.container == "ubuntu:latest" then if find("~{sep=',' files}", "/mnt/task/inputs") != None then "ok!" else "bad :(" else "ok!" }'

        # Test for the `sep` function
        echo '~{if task.container == "ubuntu:latest" then if find(sep(',', files), "/mnt/task/inputs") != None then "ok!" else "bad :(" else "ok!" }'

        # Test for the `prefix` function
        echo '~{if task.container == "ubuntu:latest" then if find(sep(',', prefix("foo", files)), "/mnt/task/inputs") != None then "ok!" else "bad :(" else "ok!" }'

        # Test for the `quote` function
        echo '~{if task.container == "ubuntu:latest" then if find(sep(',', quote(files)), "/mnt/task/inputs") != None then "ok!" else "bad :(" else "ok!" }'

        # Test for the `squote` function
        echo '~{if task.container == "ubuntu:latest" then if find(sep(',', squote(files)), "/mnt/task/inputs") != None then "ok!" else "bad :(" else "ok!" }'

        # Test for the `suffix` function
        echo '~{if task.container == "ubuntu:latest" then if find(sep(',', suffix("bar", files)), "/mnt/task/inputs") != None then "ok!" else "bad :(" else "ok!" }'

        # Test for string concatenation
        echo '~{if task.container == "ubuntu:latest" then if find("test" + files[1], "/mnt/task/inputs") != None then "ok!" else "bad :(" else "ok!" }'

        # Ensure we can read each file
        cat '~{files[0]}'
        cat '~{files[1]}'
        cat '~{files[2]}'
    >>>

    output {
        String out = read_string(stdout())
    }
}
