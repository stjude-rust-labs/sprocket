version 1.3

enum Color {
  Red = "#FF0000",
  Green = "#00FF00",
  Blue = "#0000FF"
}

workflow enum_serialization {
  input {
    Color color = Color.Red
  }

  # Test write_json serializes to bare choice name
  File color_json = write_json(color)

  # Test read_json deserializes from bare choice name
  Color color_back = read_json(color_json)

  # Test with array of enums using write_json/read_json
  Array[Color] colors = [Color.Red, Color.Green, Color.Blue]
  File colors_json = write_json(colors)
  Array[Color] colors_back = read_json(colors_json)

  # Test with map of enums
  Map[String, Color] color_map = {"primary": Color.Red, "secondary": Color.Blue}
  File map_json = write_json(color_map)
  Map[String, Color] map_back = read_json(map_json)

  output {
    Color deserialized = color_back
    Boolean colors_match = color == color_back
    String color_value = value(color_back)

    Array[Color] deserialized_colors = colors_back
    Boolean first_color_matches = colors[0] == colors_back[0]

    Map[String, Color] deserialized_map = map_back
    Boolean map_primary_matches = color_map["primary"] == map_back["primary"]
  }
}
