version 1.4

task sorted {
    input {
        File bbb
        String ccc
        Int? aaa
        Int ddd = 1

        validation {
            ccc != "": "ccc must not be empty",
            ddd > 0: "ddd must be positive"
        }
    }

    command <<<
    >>>

    output {
        String out = ccc

        validation {
            out != "": "out must not be empty"
        }
    }
}
