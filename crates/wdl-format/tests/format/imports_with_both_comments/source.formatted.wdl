version 1.1

# fileA 1.1
import  # fileA 1.2
    # fileA 2.1
    # fileA 2.2
    "fileA.wdl"  # fileA 2.3
    # fileA 3.1
    as  # fileA 3.2
    # fileA 4.1
    bar  # fileA 4.2
    # fileA 5.1
    alias  # fileA 5.2
    # fileA 6.1
    qux  # fileA 6.2
    # fileA 7.1
    as  # fileA 7.2
    # fileA 8.1
    Qux  # fileA 8.2
# this comment belongs to fileB
import "fileB.wdl" as foo  # also fileB
# this comment belongs to fileC
import "fileC.wdl"  # also fileC

workflow test {
}
