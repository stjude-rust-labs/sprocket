version 1.2

# TODO: Implement this

## This is a Foo struct
struct Foo {}

# hello world
#@ except: foo
## This is a Bar Struct
#@ except: bar
##

# this is an odd jumble
## I do cool things
struct Bar {}

#@ except: foo
## I am a baz struct

# hello world
#@ except: bar
##

# TODO: Implement this
## I do cool things
struct Baz {}