## This is a test of the `#@ except` comments.

#@ except: Unknown

version 1.1

# This applies only to the struct and everything in it
#@ except: PascalCase, SnakeCase
struct OK {         # OK
    Int AlsoOk      # OK
    Int OKTOO       # OK
}

# This applies to the specified members only
struct Ok {         # OK
    #@ except: AlsoUnknown, SnakeCase
    Int AlsoOk      # OK
    Int NotOk       # NOT OK
}

#@ except: MetaSections
workflow test {
    String bad = 'bad string'   # NOT OK
    String good =
        #@ except: DoubleQuotes
        'good string'           # OK
}
