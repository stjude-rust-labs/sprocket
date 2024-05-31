use clap::Parser;
use wdl::ast::v1::lint as ast_lint;
use wdl::core::concern::lint::Rule;
use wdl::grammar::v1::lint as grammar_lint;

/// Arguments for the `explain` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The rule name to explain.
    #[arg(required = true)]
    pub rule_name: String,
}

/// TODO IDK how to get this fn signature to work with both ast_lint and
/// grammar_lint
pub fn pretty_print_rule<E>(rule: &dyn Rule<E>) {
    println!("{}", rule.name());
    println!(
        "{}::{}",
        rule.code(),
        rule.tags(),
        // rule.level(), // TODO: Add level() to Rule trait
    );
    println!();
    println!("{}", rule.body());
}

pub fn explain(args: Args) -> anyhow::Result<()> {
    let name = args.rule_name;

    let rule = grammar_lint::rules()
        .into_iter()
        .find(|rule| rule.name() == name);

    match rule {
        Some(rule) => {
            // pretty_print_rule(&rule);
            println!("{}", rule.name());
            println!("{}::{}", rule.code(), rule.tags(),);
            println!();
            println!("{}", rule.body());
        }
        None => {
            let rule = ast_lint::rules()
                .into_iter()
                .find(|rule| rule.name() == name);

            match rule {
                Some(rule) => {
                    // pretty_print_rule(&rule);
                    println!("{}", rule.name());
                    println!("{}::{}", rule.code(), rule.tags(),);
                    println!();
                    println!("{}", rule.body());
                }
                None => {
                    anyhow::bail!("No rule found with the name '{}'", name);
                }
            }
        }
    }

    Ok(())
}
