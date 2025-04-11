#@ except: InputSorted, ParameterMetaMatched, MatchingOutputMeta, ExpectedRuntimeKeys
#@ except: Whitespace

version 1.1

task foo {
    #@ except: MetaDescription
    meta {}

    parameter_meta{}

    #@ except: InputName
    input {
        Int a=- 1
        Int w = 1
        Int x = 2
        Int y = 3
        Int z = 4
        Int f = 5
        Int b = 6
        Int complex_value = w -x +( y* ( z /(f %b) ))
        Boolean complicated_logic = (
            if !(
                a && b || c && (!d || !e)
                == (
                    if foobar
                    then gak
                    else caz
                )
            )
            then "wow"
            else "WOWOWOW"
        )
        Boolean complicated_logic2
            = (
                if
                    !(
                        a
                        && b
                        || c
                        && (
                            !d
                            ||!e
                        )
                        == (
                            if
                                foobar
                            then
                                gak
                            else
                                caz
                        )
                    )
                then
                    "wow"
                else
                    "WOWOWOW"
            )
        Boolean v = if 
        a < b then true
        else false
        Int k = (  # a comment
            2 * 5
        )
        Boolean l = (  # a comment
            if a < b then true
            else false
        )
        Boolean m = (
            # a comment
            if a < b then true
            else false
        )
        Boolean n = (
            # OK comment
            if a < b  # OK comment
            then true  # OK comment
            else false  # OK comment
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
        Array[Int] p = [1,
        2,3,]
    }

    command <<< >>>

    #@ except: OutputName
    output {
        Boolean b = ! a
    }

    runtime {}
}
