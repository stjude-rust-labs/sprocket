#@ except: BashSetSyntax, RequirementsSection, MetaSections, InputName, OutputName, DeclarationName, EmptyOutputs

version 1.3

struct ContainsStringMap {
    Map[String, Int] values
}

task foo {
    input {
        Map[String, Int] inputs
    }

    Map[String, Int] private_decl = {}
    Map[String, Int]? private_decl_optional = None

    command <<<>>>

    output {
        Map[String, Int] outputs = {}
        # Should be ignored since its type is inherited from `private_decl`
        Map[String, Int] inherited = private_decl
    }
}

task bar {
    input {
        Pair[Pair[Pair[Map[String, Int], Int], Int], Pair[Int, Int]] complex
    }

    Array[Pair[Map[String, Int], Int]] less_crazy = []

    Map[String, Map[String, Int]] map_of_maps = {}

    command <<<>>>
}

task inherited {
    input {
        Map[String, Int] existing_map
    }

    Array[Map[String, Int]] from_arr = [existing_map]
    Map[String, Int] from_index = from_arr[0]

    command <<<>>>
}

task special_cases {
    input {
        Array[Pair[String, Int]] pairs
        Map[String, Int] existing_map
    }

    # Flag any `Map[String, ...]`-producing functions, since they can be coerced
    Map[String, Int] from_as_map = as_map(pairs)

    # The array contains a fresh map literal, so still produce a diagnostic
    Array[Map[String, Int]] from_mixed_arr = [existing_map, {}]

    command <<<>>>
}

#@ except: StructOverMap
task excepted {
    input {
        Map[String, Int] inputs
    }

    Map[String, Int] private_decl = {}

    command <<<>>>

    output {
        Map[String, Int] outputs = {}
    }
}

task excepted2 {
    input {
        #@ except: StructOverMap
        Map[String, Int] inputs
    }

    #@ except: StructOverMap
    Map[String, Int] private_decl = {}

    command <<<>>>

    output {
        #@ except: StructOverMap
        Map[String, Int] outputs = {}
    }
}
