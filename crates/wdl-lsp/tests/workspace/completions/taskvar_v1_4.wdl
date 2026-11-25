version 1.4

task A {
    command <<<
        echo "~{task.}"
    >>>

    output {
        String s = task.
    }
}
