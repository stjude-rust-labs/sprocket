version 1.0

workflow test_none_literal {
  input {
    String value
  }

  # In version 1.0, `None` should parse as a name reference expression.
  # This should error as `None` is not defined anywhere
  String? maybe_value = if (value == "match") then value else None

  output {
    String? result = maybe_value
  }
}
