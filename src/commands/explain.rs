use clap::Parser;
use wdl::ast::v1::lint as ast_lint;
use wdl::grammar::v1::lint as grammar_lint;

/// Arguments for the `explain` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The rule name to explain.
    #[arg(required = true)]
    pub rule_name: String,
}

pub fn explain(args: Args) -> anyhow::Result<()> {
    let name = args.rule_name;

    let rules = grammar_lint::rules()
        .iter()
        .map(|rule| rule.name())
        .chain(ast_lint::rules().iter().map(|rule| rule.name()))
        .collect::<Vec<String>>();

    if !rules.contains(&name) {
        return Err(anyhow::anyhow!("Unknown rule: {}", name));
    }

    let rule = grammar_lint::rules()
        .into_iter()
        .find(|rule| rule.name() == name);

    match rule {
        Some(rule) => {
            println!("{}", rule.body());
        }
        None => {
            let rule = ast_lint::rules()
                .into_iter()
                .find(|rule| rule.name() == name)
                .unwrap();
            println!("{}", rule.body());
        }
    }

    Ok(())
}
