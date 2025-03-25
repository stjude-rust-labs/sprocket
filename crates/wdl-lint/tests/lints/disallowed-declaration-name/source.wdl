#@ except: MissingRequirements, InputSorting, NonmatchingOutput, MissingMetas

version 1.2

task test_declaration_names {
    meta {
        description: "This is a test of disallowed declaration names"
    }

    #@ except: SnakeCase
    input {
        # BAD
        Array[Int] arrayData
        Boolean bool_flag
        Float floatNumber
        Int my_int
        Directory dir
        Directory reference_directory

        # GOOD
        Int intermittent
        File Interval
        String name
        String name_str
        String nameString
        Directory direct_descendant
    }

    # BAD
    Int count_int = 42
    Int result_integer = 42

    # GOOD
    String name_string = "test"
    Int foo_bar_InT = 42  # Split by convert-case to [foo, bar, In, T]
    # Split by convert-case to [foo, bar, INT]
    # and INT will not flag as we don't call `to_lowercase()` on the split words.
    Int foo_bar_INT = 42

    command <<<>>>

    #@ except: SnakeCase
    output {
        # BAD
        Int result_int = 42
        # GOOD
        File file = "output.txt"
        String resultString = "result"
    }

    runtime {}
}
