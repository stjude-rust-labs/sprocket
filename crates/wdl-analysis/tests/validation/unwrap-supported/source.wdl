## This is a test of the unwrap operator being supported in WDL 1.4

#@ except: UnusedDeclaration

version 1.4

workflow test {
    Int? x = 1
    Int a = x!
    Boolean b = x! == 1
}
