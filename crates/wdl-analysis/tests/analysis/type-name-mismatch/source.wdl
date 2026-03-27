## This is a test to ensure users can differentiate between
## a struct type and a name reference to the struct.

version 1.2

struct Resource {
    Int x
}

workflow test {
    #@ except: UnusedDeclaration
    Resource x = Resource
}
