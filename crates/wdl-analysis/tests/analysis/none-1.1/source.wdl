version 1.1

workflow test_none_literal {
  input {
    String value
  }

  # In version 1.1+, `None` should parse as a literal none expression
  # This should produce no errors
  String? maybe_value = if (value == "match") then value else None

  output {
    String? result = maybe_value
  }
}
