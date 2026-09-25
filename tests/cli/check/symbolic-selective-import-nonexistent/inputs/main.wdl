version 1.4

import { brew_chai } from coffeeshop

workflow make_coffee {
    call brew_chai
}
