version 1.3

import "external.wdl" alias ExternalPerson as AliasedPerson alias ExternalColor as AliasedColor

workflow foo {
    AliasedPerson p = AliasedPerson { name: "Jane" }
    String n = p.name
    AliasedColor c = AliasedColor.Green
}
