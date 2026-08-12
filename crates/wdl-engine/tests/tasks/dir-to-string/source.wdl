version 1.3

# https://github.com/stjude-rust-labs/sprocket/issues/815
#
# The `Directory` -> `String` conversion should drop trailing slashes
task dir_to_string {
    input {
        Directory dir
    }

    command <<<
        echo "~{dir}"
        mkdir "some_dir"
    >>>

    output {
        Directory dir_to_string = "some_dir/"
    }
}
