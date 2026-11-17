version 1.0

workflow main {

    output {
        String s = read_string()
        String sub = sub("a", "b", )
        Float sz = size()
        Array[String] parts = split("a,b", ",")
    }
}
