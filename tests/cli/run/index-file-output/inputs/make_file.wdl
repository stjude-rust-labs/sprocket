version 1.3

task make_file {
    command <<<
        echo "hello" > greeting.txt
    >>>

    output {
        File greeting = "greeting.txt"
    }
}
