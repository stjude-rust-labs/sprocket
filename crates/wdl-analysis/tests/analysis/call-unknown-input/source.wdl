## This is a test of having an unknown input in a call statement.

version 1.2

task my_task {
    command <<<>>>
}

workflow test {
    call my_task { input: x = 1 }
    call my_task as my_task2 { x = 1 }
    call my_task as my_task3 { input: x }
    call my_task as my_task4 { x }
}
