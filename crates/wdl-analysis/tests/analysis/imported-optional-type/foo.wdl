version 1.1

task foo {
    input {
        #@ except: UnusedInput
        Array[String]? foo
    }

    command <<<>>>
}
