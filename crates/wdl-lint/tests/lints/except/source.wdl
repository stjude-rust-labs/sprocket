#@ except: Unknown
## This is a test of the `#@ except` comments.

version 1.1

# This applies only to the struct and everything in it
#@ except: PascalCase, SnakeCase
struct OK {         # OK
    Int AlsoOk      # OK
    Int OKTOO       # OK
}

# This applies to the specified members only
struct Ok {         # OK
    #@ except: SnakeCase,AlsoUnknown
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
