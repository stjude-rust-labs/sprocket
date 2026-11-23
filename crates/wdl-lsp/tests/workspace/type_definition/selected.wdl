version 1.4

import { ExternalPerson as SelectedPerson, greet_external } from "external.wdl"

workflow foo {
    SelectedPerson p = SelectedPerson { name: "Jamie" }
    call greet_external {
        person = p
    }
    SelectedPerson res = greet_external.out
}
