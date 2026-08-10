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
