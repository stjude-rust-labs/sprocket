version 1.0
import "fileB.wdl" as foo # fileB
workflow test {}
import "fileC.wdl" # fileC
import # fileA 1
"fileA.wdl" # fileA 2
as # fileA 3
bar # fileA 4
alias # fileA 5
qux # fileA 6
as # fileA 7
Qux # fileA 8
