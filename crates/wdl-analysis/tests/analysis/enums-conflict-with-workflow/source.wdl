## Tests that enum names may conflict with workflow names.
version 1.3

# Enum with same name as workflow
enum test {
    Active,
    Pending
}

workflow test {}
