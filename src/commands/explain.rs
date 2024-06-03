use clap::Parser;
use colored::Colorize;
use wdl::ast::v1::lint as ast_lint;
use wdl::core::concern::lint::Rule;
use wdl::grammar::v1::lint as grammar_lint;

/// Arguments for the `explain` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The name or code of the rule to explain.
    #[arg(required = true)]
    pub rule_identifier: String,
}

pub fn pretty_print_rule<E>(rule: &dyn Rule<E>) {
    println!("{}", rule.name().bold().underline());
    println!("{}", format!("{}::{}", rule.code(), rule.tags(),).yellow());
    println!();
    println!("{}", rule.body());
}

pub fn explain(args: Args) -> anyhow::Result<()> {
    let ident = args.rule_identifier;

    let rule = grammar_lint::rules()
        .into_iter()
        .find(|rule| rule.name() == ident || rule.code().to_string() == ident);

    match rule {
        Some(rule) => {
            pretty_print_rule(&*rule);
        }
        None => {
            let rule = ast_lint::rules()
                .into_iter()
                .find(|rule| rule.name() == ident || rule.code().to_string() == ident);

            match rule {
                Some(rule) => {
                    pretty_print_rule(&*rule);
                }
                None => {
                    anyhow::bail!("No rule found with the identifier '{}'", ident);
                }
            }
        }
    }

    Ok(())
}
