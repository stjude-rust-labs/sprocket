## This test ensures that enum variant values must be literal expressions only.

version 1.3

# This should fail - using a computed expression
enum InvalidComputed {
    Value = 1 + 1
}

# This should fail - using string interpolation
enum InvalidInterpolation {
    Path = "/tmp/~{some_var}"
}

# This should fail - using a variable reference
enum InvalidReference {
    Default = some_value
}
