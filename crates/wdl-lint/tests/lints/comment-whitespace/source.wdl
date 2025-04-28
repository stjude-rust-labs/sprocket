#@ except: ExpressionSpacing, ElementSpacing

version 1.2

#a bad comment
    # another bad comment

# a good comment
# a comment with trailing whitespace          

#@ except: MetaSections, MatchingOutputMeta
workflow foo {# test in-line comment without preceding whitespace
    #@ except: MetaDescription
    meta {# this is a problematic comment
    }

    input { # a bad comment
        String foo  # a good comment
    # another bad comment
            # yet another bad comment
        String baz = "baz"       # too much space for an inline comment
    }

    #@ except: OutputName
    output {  # a fine comment
              # what about this one?

        # an OK comment
        String bar = foo

        Int a = 5 /
            # a comment
            10
            / (
                # a b comment
                2
            )
            /
            # another comment
            2
        Int b = 5 / (  # yet another comment
            (  # more comment
                # even more comment
                2 * 5
            )
        )
        Int c = 5 / ((  # more comment
                # even more comment
                2 * 5
            )
        )
        Int d = 5 / (
            # even more comment
            2 * 5
        )
        Int e = 2
            * (
                # comment
                2
            )
        Int f = (
            # comment
            2
        )
        * 2
        Int g = 2 *
            (
                # comment
                2
            )
        Boolean h = [1,2,3] == [1,2,3]
        Boolean i = [1
            # a comment
            ,2,3,] == [1,2,4]
        Boolean j = [
            1,
            2,
            3,
            # a comment
        ]
        == [
            # comment
            1,
            2,
            3,
        ]
        Boolean q = [
            1,
            2,
            3,
            # a comment
        ]
        ==
        [
            # This comment will flag, because the  `] == [` expression is incorrect.
            1,
            2,
            3,
        ]
        Boolean k = {"a": 1, "b": 2} == {"b": 2, "a": 1}
        Boolean l = {
            # comment
            "a": 1,
            "b": 2,
        } == {
            "b": 2,
            "a": 1,
            # comment
        }
        Boolean m = {
            # comment
            "a": 1,
            "b": 2,
        }
        == {
            "b": 2,
            "a": 1,
            # comment
        }
        Boolean n = {
            # comment
            "a": 1,
            "b": 2,
        }
        ==
        {
            "b": 2,
            "a": 1,
            # This comment will flag, because the  `} == {` expression is incorrect.
        }
        Boolean o = {
            # comment
            "a": 1,
            "b": 2,
        } == {
            "b": 2,
            "a": 1,
            # This comment is OK.
        }
    }
}
