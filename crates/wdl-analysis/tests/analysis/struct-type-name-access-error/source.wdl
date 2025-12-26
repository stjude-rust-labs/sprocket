version 1.3

struct Person {
  String name
  Int age
}

workflow struct_type_name_access_error {
  # This should fail, as we cannot access struct types like enum types
  #@ except: UnusedDeclaration
  String x = Person.name
}
