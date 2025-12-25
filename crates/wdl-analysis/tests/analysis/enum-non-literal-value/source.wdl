## This test ensures that enum variant values must be literal expressions only.

version 1.3

# Valid examples with literal values:

# This should pass: integer literals
enum ValidIntegers[Int] {
    Small = 1,
    Medium = 50,
    Large = 100
}

# This should pass: string literals
enum ValidStrings[String] {
    Hello = "hello",
    World = "world",
    Message = "test"
}

# This should pass: array with literal elements
enum ValidArray[Array[Int]] {
    SmallNumbers = [1, 2, 3],
    LargeNumbers = [100, 200, 300]
}

# This should pass: pair with literal values
enum ValidPair[Pair[Int, String]] {
    First = (1, "one"),
    Second = (2, "two")
}

# This should pass: map with literal keys and values
enum ValidMap[Map[String, Int]] {
    Counts = {"a": 1, "b": 2, "c": 3},
    Scores = {"high": 100, "low": 10}
}

# This should pass: object with literal member values
enum ValidObject[Object] {
    Config = object {port: 8080, host: "localhost"},
    Settings = object {timeout: 30, retries: 3}
}

# This should pass: nested compound types with all literals
enum ValidNested[Array[Array[Int]]] {
    Matrix = [[1, 2], [3, 4], [5, 6]],
    Grid = [[10, 20], [30, 40]]
}

struct Endpoint {
    String host
    Int port
}

# This should pass: struct with literal member values
enum ValidStruct[Endpoint] {
    Local = Endpoint {host: "localhost", port: 8080},
    Remote = Endpoint {host: "example.com", port: 443}
}

# Primitive non-literals:

# This should fail: using a computed expression
enum InvalidComputed {
    Value = 1 + 1
}

# This should fail: using string interpolation
enum InvalidInterpolation {
    Path = "/tmp/~{some_var}"
}

# This should fail: using a variable reference
enum InvalidReference {
    Default = some_value
}

# Compound types with non-literal elements:

# This should fail: array with computed expression
enum InvalidArrayComputed {
    Values = [1, 2, 1 + 1]
}

# This should fail: array with variable reference
enum InvalidArrayReference {
    Values = [1, 2, some_var]
}

# This should fail: array with string interpolation
enum InvalidArrayInterpolation {
    Paths = ["/tmp/a", "/tmp/~{b}"]
}

# This should fail: pair with computed expression
enum InvalidPairComputed {
    Value = (1, 1 + 1)
}

# This should fail: pair with variable reference
enum InvalidPairReference {
    Value = (1, some_var)
}

# This should fail: pair with string interpolation
enum InvalidPairInterpolation {
    Value = ("a", "~{b}")
}

# This should fail: map with computed key
enum InvalidMapComputedKey {
    Value = {1 + 1: "value"}
}

# This should fail: map with computed value
enum InvalidMapComputedValue {
    Value = {1: 1 + 1}
}

# This should fail: map with variable reference in key
enum InvalidMapReferenceKey {
    Value = {some_var: "value"}
}

# This should fail: map with variable reference in value
enum InvalidMapReferenceValue {
    Value = {1: some_var}
}

# This should fail: map with string interpolation in key
enum InvalidMapInterpolationKey {
    Value = {"~{key}": "value"}
}

# This should fail: map with string interpolation in value
enum InvalidMapInterpolationValue {
    Value = {"key": "~{value}"}
}

# This should fail: object with computed value
enum InvalidObjectComputed {
    Value = object {a: 1 + 1}
}

# This should fail: object with variable reference
enum InvalidObjectReference {
    Value = object {a: some_var}
}

# This should fail: object with string interpolation
enum InvalidObjectInterpolation {
    Value = object {a: "~{value}"}
}

# Nested compound types with non-literals:

# This should fail: array of arrays with computed expression
enum InvalidNestedArrayComputed {
    Values = [[1, 2], [3, 1 + 1]]
}

# This should fail: array of pairs with variable reference
enum InvalidNestedArrayPairReference {
    Values = [(1, 2), (3, some_var)]
}

# This should fail: map of maps with computed value
enum InvalidNestedMapComputed {
    Values = {"outer": {1: 1 + 1}}
}

# This should fail: object with array containing variable
enum InvalidNestedObjectArrayReference {
    Value = object {items: [1, 2, some_var]}
}
