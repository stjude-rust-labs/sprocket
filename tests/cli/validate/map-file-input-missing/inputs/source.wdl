version 1.0

workflow compound_map_checks {
    input {
        Map[String, File] named_files
    }

    output {
        Map[String, File] echoed = named_files
    }
}
