#@ except: ElementSpacing, MetaDescription

## This is a test to check import order

version 1.1

import "A.wdl"
import "B.wdl"
import "D.wdl"
import "C.wdl"

workflow test {
    meta {}
    output {}
}
