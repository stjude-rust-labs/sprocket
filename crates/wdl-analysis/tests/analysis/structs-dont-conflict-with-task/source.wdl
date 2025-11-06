version 1.2

struct MyTask {
    String field
}

task MyTask {
    command <<<
        echo "test"
    >>>

    output {
        String result = read_string(stdout())
    }
}
