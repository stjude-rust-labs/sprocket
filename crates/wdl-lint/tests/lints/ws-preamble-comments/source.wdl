#@ except: BlankLinesBetweenElements, DescriptionMissing
## This is a test of whitespace between preamble comments

  
## The above lines should be a warning, as well as below

## Last comment

version 1.1

workflow test {
    meta {}
    output {}
}
