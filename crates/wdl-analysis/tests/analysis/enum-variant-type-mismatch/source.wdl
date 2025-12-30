version 1.3

# Primitive type mismatch: String vs Int
enum Status {
    Active = "active",
    Pending = 42
}

# Array type mismatch: Array[Int] vs Array[String]
enum DataSets {
    Numbers = [1, 2, 3],
    Strings = ["a", "b", "c"]
}

# Map type mismatch: Map[String, Int] vs Map[String, String]
enum Config {
    Ports = {"http": 80, "https": 443},
    Names = {"first": "Alice", "last": "Bob"}
}

# Pair type mismatch: Pair[Int, String] vs Pair[String, Int]
enum Coords {
    LatLon = (37, "N"),
    LonLat = ("122W", 37)
}

# Mixed types within variants
enum Mixed {
    First = 1,
    Second = "two",
    Third = 3.0
}

workflow test {}
