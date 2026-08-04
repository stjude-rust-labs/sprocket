version 1.0

workflow test_none_literal {
  input {
    String value
  }

  # In version 1.0, `None` should parse as an name reference expression
  String? maybe_value = if (value == "match") then value else None

  output {
    String? result = maybe_value
  }
}
