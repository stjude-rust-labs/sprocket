#@ except: BlankLinesBetweenElements, DescriptionMissing

## This is a test to check import order

version 1.1

import "A"
import "B"
import "D"
import "C"

workflow test {
    meta {}
    output {}
}
