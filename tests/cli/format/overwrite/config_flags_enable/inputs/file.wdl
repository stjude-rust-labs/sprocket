version 1.2

import "z.wdl" as Z
import "a.wdl" as A

workflow sample {
    input {
        Int zebra
        String alpha
    }

    Array[String] values = [
        "one",
        "two"
    ]
}
