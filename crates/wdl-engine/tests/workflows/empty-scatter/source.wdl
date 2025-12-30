## This is a test of a scatter evaluation on an empty array

version 1.1

task t {
    command <<<>>>

    output {
        String s = "hi"
    }
}

workflow w {
    scatter (i in []) {
        call t as a
        Int b = 1
    }

    output {
        # These outputs should be empty arrays
        Array[String] c = a.s
        Array[Int] d = b
    }
}
