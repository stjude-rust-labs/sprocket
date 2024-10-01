# This is a test of passing an incorrect type to a call input.

version 1.2

task my_task {
    input {
        Int x
    }

    command <<<>>>
}

workflow test {
    String x = "1"

    call my_task { input: x = "1" }
    call my_task as my_task2 { x = x }
    call my_task as my_task3 { x }
    call my_task as my_task4 { input: x }
}
