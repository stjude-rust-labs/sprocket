version 1.2

workflow main {

    output {
        String s = read_string()
        String sub = sub("a", "b", )
        Float sz = size()
    }
}
