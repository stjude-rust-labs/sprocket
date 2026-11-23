version 1.4

task make_file {
    command <<<
        echo "hello" > greeting.txt
    >>>

    output {
        File greeting = "greeting.txt"
    }
}
