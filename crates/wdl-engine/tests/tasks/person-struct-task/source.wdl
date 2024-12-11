version 1.2

struct Name {
  String first
  String last
}

struct Income {
  Int amount
  String period
  String? currency
}

struct Person {
  Name name
  Int age
  Income? income
  Map[String, File] assay_data
  
  meta {
    description: "Encapsulates data about a person"
  }

  parameter_meta {
    name: "The person's name"
    age: "The person's age"
    income: "How much the person makes (optional)"
    assay_data: "Mapping of assay name to the file that contains the assay data"
  }
}

task greet_person {
  input {
    Person person
  }

  Array[Pair[String, File]] assay_array = as_pairs(person.assay_data)

  command <<<
  printf "Hello ~{person.name.first}! You have ~{length(assay_array)} test result(s) available.\n"

  if ~{defined(person.income)}; then
    if [ "~{select_first([person.income]).amount}" -gt 1000 ]; then
      currency="~{select_first([select_first([person.income]).currency, "USD"])}"
      printf "Please transfer $currency 500 to continue"
    fi
  fi
  >>>

  output {
    String message = read_string(stdout())
  }
}
