#@ except: UnusedDeclaration, UnusedCall
## This is a test of accessing an unknown call output.

version 1.1

task my_task {
    command <<<>>>
}

workflow test {
    call my_task
    String x = my_task.unknown
}
