version 1.3

# CyclicEnum <-> CyclicStruct
enum CyclicEnum[CyclicStruct] {}

struct CyclicStruct {
    CyclicEnum cycle
}

# CyclicEnum2 <-> CyclicEnum2
enum CyclicEnum2[Array[CyclicEnum2]] {
    Cycle = [CyclicEnum2.Cycle]
}

# CyclicEnum3 <-> CyclicEnum4
enum CyclicEnum3[Array[CyclicEnum4]] {
    Cycle = [CyclicEnum4.Cycle]
}

enum CyclicEnum4[Array[CyclicEnum3]] {
    Cycle = [CyclicEnum3.Cycle]
}

# CyclicEnum5 <-> CyclicStruct3
enum CyclicEnum5[Map[String, CyclicStruct3]] {}

struct CyclicStruct3 {
    CyclicEnum5 cycle
}

# Non-cyclic
enum NonCyclicEnum {}

struct NonCyclicStruct {
    NonCyclicEnum non_cyclic
}