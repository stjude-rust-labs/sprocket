version 1.4

import * from coffeeshop

workflow make_coffee {
    call grind_beans {
        input:
            roast_level = "dark"
    }

    output {
        String result = grind_beans.ground_coffee
    }
}
