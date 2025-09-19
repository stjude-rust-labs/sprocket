version 1.1

# fileA 1
import
    # fileA 2.1
    # fileA 2.2
    "fileA.wdl"
    # fileA 3
    as
    # fileA 4
    bar
    # fileA 5
    alias
    # fileA 6
    qux
    # fileA 7
    as
    # fileA 8
    Qux
# this comment belongs to fileB
import "fileB.wdl" as foo
# this comment belongs to fileC
import "fileC.wdl"

workflow test {
}
