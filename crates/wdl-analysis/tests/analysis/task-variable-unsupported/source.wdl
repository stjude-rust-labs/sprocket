## This is a test of using a task variable in an unsupported WDL version.

version 1.1

task test {
    command <<<
        echo "Hello from ~{task.name}!"
    >>>

    output {
        String name = task.name
    }
}
