version 1.2

task A {
    command <<<
        echo "~{task.}"
    >>>

    output {
        String s = task.
    }
}
