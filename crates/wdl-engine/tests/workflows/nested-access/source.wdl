version 1.2

struct Experiment {
  String id
  Array[String] variables
  Map[String, String] data
}

workflow nested_access {
  input {
    Array[Experiment]+ my_experiments
  }

  Experiment first_experiment = my_experiments[0]
  
  output {
    # these are equivalent
    String first_var = first_experiment.variables[0]
    String first_var_from_first_experiment = my_experiments[0].variables[0]

    # these are equivalent
    String subject_name = first_experiment.data["name"]
    String subject_name_from_first_experiment = my_experiments[0].data["name"]
  }
}