## This is a test of `read_tsv` overloads resolving their return types statically.

version 1.3

task read_tsv_task {
    input {
        File tsv
        Boolean has_header = true
    }

    command <<<>>>

    output {
        Array[Array[String]] table = read_tsv(tsv)
        Array[Object] with_variable = read_tsv(tsv, has_header)
        Array[Object] with_true = read_tsv(tsv, true)
        Array[Object] with_false = read_tsv(tsv, false)
        Array[Object] with_columns = read_tsv(tsv, false, ["name", "value"])

        # The two-argument form always returns `Array[Object]`, even for `false`
        Array[Array[String]] mismatch = read_tsv(tsv, false)
    }
}
