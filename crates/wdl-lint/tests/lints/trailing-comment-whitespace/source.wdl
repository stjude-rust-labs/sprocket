#@ except: BlankLinesBetweenElements       

## The above lint directive has trailing whitespace
## This is a preamble comment with whitespace trailing 

version 1.1

# This is a workflow comment with trailing whitespace 
workflow test {
    # Next is a lint directive with trailing whitespace
    #@ except: DescriptionMissing       
    meta {}
    parameter_meta {}
    output {}
}
