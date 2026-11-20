## This is a test of the unwrap operator being unsupported prior to WDL 1.4

#@ except: UnusedDeclaration

version 1.3

workflow test {
    Int? x = 1
    Int a = x!
    Boolean b = x! == 1
}
