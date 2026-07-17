version 1.2

import "a.wdl" as A
import "z.wdl" as Z

workflow sample {
    input {
        String alpha
        Int zebra
    }

    Array[String] values = [
        "one",
        "two",
    ]
}
