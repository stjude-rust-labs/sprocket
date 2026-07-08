version 1.4

import * from shapes

workflow w {
    input {
        Point p
        Status s
    }

    output {
        Int total = p.x + p.y
        Status chosen = s
    }
}
