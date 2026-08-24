version 1.2

workflow literal_array_inlining {
    # An array that fits on the line it starts on is written on that line.
    Array[Int] fits = [1, 2, 3]

    # This one ends exactly at the maximum line length.
    Array[String] barely_fits = ["alpha", "bravo", "charlie", "delta", "echo", "november"]

    # One column wider, so it is written with an element per line instead.
    Array[String] barely_over = [
        "alpha",
        "bravo",
        "charlie",
        "delta",
        "india",
        "november",
    ]

    # The starting column counts too, so the array that fits above does not here.
    if (true) {
        Array[String] deeper_copy = [
            "alpha",
            "bravo",
            "charlie",
            "delta",
            "echo",
            "november",
        ]
    }

    # So does whatever follows the closing bracket, comments included.
    Array[Int] with_short_comment = [1, 2, 3]  # fits
    Array[Int] with_long_comment = [
        1,
        2,
        3,
    ]  # this comment leaves the array no room at all

    # A nested array is measured with the delimiters and separators around it.
    Array[Int] nested = flatten([[1, 2, 3], [4, 5, 6]])

    # A map, object, or struct literal element is always written vertically.
    Array[Map[String, Int]] maps = [
        {
            "a": 1,
        },
    ]

    # A run of comment lines
    # above the declaration
    # is not part of the array's line.
    Array[Int] after_comments = [1, 2, 3]

    # A comment anywhere in the array keeps it multiline.
    Array[Int] commented = [
        1,  # one
        2,
    ]

    Array[Int] leading_comment = [
        # first
        1,
        2,
    ]

    Array[Int] empty = []

    output {
        Array[Int] out = fits
    }
}
