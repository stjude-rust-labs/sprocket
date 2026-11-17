## This is a test that doc comments on all struct members silence the
## `MetaSections` lint's `parameter_meta` requirement.

version 1.3

## A struct to represent the filtering flags used in various `samtools` commands.
##
## The order of precedence is `include_if_all`, `exclude_if_any`, `include_if_any`,
## and `exclude_if_all`.
struct FlagFilter {
    ## Corresponds to `samtools -f`
    String include_if_all
    ## Corresponds to `samtools -F`
    String exclude_if_any
    ## Corresponds to `samtools --rf`
    String include_if_any
    ## Corresponds to `samtools -G`
    String exclude_if_all
}