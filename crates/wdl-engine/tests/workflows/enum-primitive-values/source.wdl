version 1.3

enum Status {
  Inactive = 0,
  Active = 1,
  Pending = 2
}

enum Color {
  Red = "#FF0000",
  Green = "#00FF00",
  Blue = "#0000FF"
}

workflow enum_primitive_values {
  output {
    Status inactive = Status.Inactive
    Status active = Status.Active
    Status pending = Status.Pending
    Int inactive_value = value(Status.Inactive)
    Int active_value = value(Status.Active)
    Int pending_value = value(Status.Pending)
    Boolean inactive_equals_inactive = Status.Inactive == Status.Inactive
    Boolean inactive_not_equals_active = Status.Inactive != Status.Active

    Color red = Color.Red
    Color green = Color.Green
    Color blue = Color.Blue
    String red_value = value(Color.Red)
    String green_value = value(Color.Green)
    String blue_value = value(Color.Blue)
    Boolean red_equals_red = Color.Red == Color.Red
    Boolean red_not_equals_blue = Color.Red != Color.Blue

    # Test parenthesized enum type name access
    Status paren_inactive = (Status).Inactive
    Status paren_active = (Status).Active
    Color paren_red = (Color).Red
    Color paren_blue = (Color).Blue
  }
}
