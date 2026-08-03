version 1.0

workflow main {
    input {
        File file
    }

    # `basename` is a polymorphic function with only one variant
    # in WDL 1.0, two additional variants added in WDL 1.2.
    String base = basename(file)

    # `split` is a monomorphic function added in WDL 1.3.
    Array[String] s = split("foo bar", " ")

    Map[String, String] map = {}
    # `contains_key` is a polymorphic function added in WDL 1.2.
    Boolean b = contains_key(map, 'key')

    output {
    }
}
