version 1.2

import "../person-struct-task/source.wdl"
  alias Person as Patient
  alias Income as PatientIncome

# This struct has the same name as a struct in 'structs.wdl',
# but they have identical definitions so an alias is not required.
struct Name {
  String first
  String last
}

# This struct also has the same name as a struct in 'structs.wdl',
# but their definitions are different, so it was necessary to
# import the struct under a different name.
struct Income {
  Float dollars
  Boolean annual
}

struct Person {
  Int age
  Name name
  Float? height
  Income income
}

task calculate_bill {
  input {
    Person doctor = Person {
      age: 10,
      name: Name {
        first: "Joe",
        last: "Josephs"
      },
      income: Income {
        dollars: 140000,
        annual: true
      }
    }

    Patient patient = Patient {
      name: Name {
        first: "Bill",
        last: "Williamson"
      },
      age: 42,
      income: PatientIncome {
        amount: 350,
        currency: "Yen",
        period: "hourly"
      },
      assay_data: {
        "glucose": "hello.txt"
      }
    }

    PatientIncome average_income = PatientIncome {
      amount: 50000,
      currency: "USD",
      period: "annually"
    }
  }
  
  PatientIncome income = select_first([patient.income, average_income])
  String currency = select_first([income.currency, "USD"])
  Float hourly_income = if income.period == "hourly" then income.amount else income.amount / 2000
  Float hourly_income_usd = if currency == "USD" then hourly_income else hourly_income * 100

  command <<<
  printf "The patient makes $~{hourly_income_usd} per hour\n"
  >>>
  
  output {
    Float bill = hourly_income_usd * 5
  }
}
