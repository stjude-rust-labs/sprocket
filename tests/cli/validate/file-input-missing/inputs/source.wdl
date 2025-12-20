version 1.0

workflow file_checks {
    input {
        File sample_file
        Directory sample_dir
    }

    output {
        File echoed_file = sample_file
        Directory echoed_dir = sample_dir
    }
}
