#@ except: CommentWhitespace, Whitespace, EndingNewline, Unknown
## This is a test of the `#@ except` comments.
## The above exceptions apply to the whole file.

version 1.1

# This applies only to the struct and everything in it
#@ except: PascalCase,SnakeCase
struct OK {         # OK
    Int AlsoOk      # OK
    Int OKTOO       # OK
}


# Intentional extraneous whitespace lines that should not be a warning
# because we've excepted the rule for the entire document


# This applies to the specified members only
struct Ok {         # OK
    #@ except: SnakeCase,AlsoUnknown
    Int AlsoOk      # OK
    Int NotOk       # NOT OK
}

#@ except: MissingMetas,MissingOutput,Whitespace
workflow test {
    String bad = 'bad string'   # NOT OK
    #@ except: DoubleQuotes
    String good =
        'good string'           # OK
}


# Lots of trailing whitespace too!




