version 1.3

workflow main {

    output {
        String s = read_string()
        String sub = sub("a", "b", )
        Float sz = size()
    }
}
