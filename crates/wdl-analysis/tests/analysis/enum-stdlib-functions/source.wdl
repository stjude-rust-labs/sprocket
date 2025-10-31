version 1.3

enum Color {
  Red = "FF0000",
  Green = "00FF00",
  Blue = "0000FF"
}

workflow test_enum_stdlib {
  # Test using enum type in declarations
  Color my_color = Color.Red

  # Test value() function - returns underlying value
  String underlying_value = value(my_color)

  # Test implicit conversion - enum to string returns name
  String implicit_name = "~{my_color}"

  output {
    String out_value = underlying_value
    String out_implicit = implicit_name
  }
}
