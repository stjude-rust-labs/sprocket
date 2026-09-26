version 1.3

# Primitive type mismatch: Boolean vs Int
enum Status {
    Active = true,
    Pending = 42,
}

# Array type mismatch: Array[Int] vs Array[Boolean]
enum DataSets {
    Numbers = [1, 2, 3],
    Booleans = [true, false, true]
}

# Map type mismatch: Map[String, Int] vs Map[String, Boolean]
enum Config {
    Ports = {
        "http": 80,
        "https": 443,
    },
    Flags = {
        "first": true,
        "last": false,
    },
}

# Pair type mismatch: Pair[Int, Boolean] vs Pair[Boolean, Int]
enum Coords {
    LatLon = (37, true),
    LonLat = (false, 37),
}

# Mixed types within choices
enum Mixed {
    First = 1,
    Second = false,
    Third = 3.0,
}

workflow test {
}
