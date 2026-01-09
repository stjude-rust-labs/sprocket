version 1.2

# Test case 1: Long URI that exceeds 90 characters
import "https://raw.githubusercontent.com/stjudecloud/workflows/refs/heads/main/tools/librarian.wdl"

# Test case 2: Import with long 'as' namespace clause
import "structs.wdl" as very_long_namespace_name_that_definitely_exceeds_the_ninety_character_maximum_line_width

# Test case 3: WDL 1.2 alias with long names
import "types.wdl"
  alias VeryLongStructName as EvenLongerAliasNameThatExceedsNinetyCharactersWhenCombinedWithEverythingElse

workflow test {
    meta {
        description: "Test workflow for import line width"
    }

    parameter_meta {}

    input {}

    output {}
}
