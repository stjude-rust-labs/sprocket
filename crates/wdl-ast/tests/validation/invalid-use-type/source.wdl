## This is a test of using the `env` keyword on a non-primitive type.

version 1.2

task test {
    input {
        env String a
        env Float b
        env Int c
        env File d
        env Directory e
        env Boolean f

        # NOT OK
        env Array[String] g
    }

    env String h = ""
    env Float i = 1.0
    env Int j = 1
    env File k = ""
    env Directory l = ""
    env Boolean m = ""

    # NOT OK
    env Array[String] n = [1, 2, 3]

    command <<<>>>
}
