## Tests that enum names may conflict with task names.
version 1.3

task Status {
    command <<<
        echo "hello"
    >>>
}

# Enum with same name as task
enum Status {
    Active,
    Pending
}

workflow test {}
