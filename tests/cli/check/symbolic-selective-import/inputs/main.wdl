version 1.4

import { grind_beans } from coffeeshop

workflow make_coffee {
    call grind_beans {
        input:
            roast_level = "dark"
    }

    output {
        String result = grind_beans.ground_coffee
    }
}
