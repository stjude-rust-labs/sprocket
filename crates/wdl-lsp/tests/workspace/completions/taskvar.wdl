version 1.3

task A {
    command <<<
        echo "~{task.}"
    >>>

    output {
        String s = task.
    }
}
