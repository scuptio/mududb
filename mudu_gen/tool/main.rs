mod test_tool;

use clap::{Arg, ArgAction, ArgMatches, Command};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_gen::src_gen::gen_entity::gen_rust;
use mudu_gen::src_gen::gen_message::gen_message;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let r = main_inner(std::env::args());
    match r {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("mgen error: {}", e);
            Err(Box::new(e))
        }
    }
}

#[derive(Debug)]
struct EntityConfig {
    pub input: Vec<String>,
    pub output: String,
    pub type_desc: Option<String>,
    pub lang: String,
}

#[derive(Debug)]
struct MessageConfig {
    pub input: String,
    pub output: String,
    pub lang: String,
    pub namespace:Option<String>,
}

fn run_gen_message(config: &MessageConfig) -> RS<()> {
    gen_message(&config.input, config.output.clone(), config.lang.clone(),config.namespace.clone())?;
    Ok(())
}

fn run_gen_entity(config: &EntityConfig) -> RS<()> {
    gen_rust(
        config.input.clone(),
        config.output.clone(),
        config.type_desc.clone(),
        config.lang.clone(),
    )?;
    Ok(())
}

// parse the arguments
fn parse_arguments<I, T>(args: I) -> RS<ArgMatches>
where
    I: IntoIterator<Item=T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let command = Command::new("mgen")
        .version("1.0")
        .author("scuptio")
        .about("Mudu Source Code Generate(mgen), generate source code from wit/sql")
        // Common arguments shared by all subcommands
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::SetTrue)
                .required(false)
                .global(true)
                .help("Enable verbose output"),
        )
        // Subcommands
        .subcommand(
            Command::new("entity")
                .about("Generate entity class from DDL SQL/wit")
                .arg(
                    Arg::new("input-source-files")
                        .short('i')
                        .long("input-source-files")
                        .value_name("FILE")
                        .required(true)
                        .num_args(1..)
                        .help("Input file path(s), can specify multiple"),
                )
                .arg(
                    Arg::new("output-source-folder")
                        .short('o')
                        .long("output-source-folder")
                        .value_name("FOLDER")
                        .required(true)
                        .help("Output source file folder path"),
                )
                .arg(
                    Arg::new("type-desc")
                        .short('t')
                        .long("type-desc")
                        .value_name("FILE")
                        .required(true)
                        .help("output type description file path"),
                )
                .arg(
                    Arg::new("lang")
                        .short('l')
                        .long("lang")
                        .value_name("LANG")
                        .required(true)
                        .help("Generate language"),
                ),
        )
        .subcommand(
            Command::new("message")
                .alias("msg")
                .about("Message data type for serialize/deserialize")
                .arg(
                    Arg::new("input-wit-file")
                        .short('i')
                        .long("input-wit-file")
                        .value_name("FILE")
                        .required(true)
                        .help("Input .wit file path"),
                )
                .arg(
                    Arg::new("output-source-file")
                        .short('o')
                        .long("output-source-file")
                        .value_name("FILE")
                        .required(true)
                        .help("Output file path"),
                )
                .arg(
                    Arg::new("lang")
                        .short('l')
                        .long("lang")
                        .value_name("LANG")
                        .global(false)
                        .required(true)
                        .help("Generate language"),
                )
                .arg(
                    Arg::new("namespace")
                        .short('n')
                        .long("namespace")
                        .value_name("NAME")
                        .global(false)
                        .help("Namespace"),
                ),
        );
    let r_matches = command
        .try_get_matches_from(args);
    let matches = match r_matches {
        Ok(matches) => {
            matches
        }
        Err(e) => {
            if e.kind() == clap::error::ErrorKind::DisplayHelp
                || e.kind() == clap::error::ErrorKind::DisplayVersion
            {
                eprintln!("{}", e);
                std::process::exit(0);
            } else {
                eprintln!("parse arguments error: \n{}", e);
                std::process::exit(0);
            }
        }
    };

    Ok(matches)
}

fn process_arguments(matches: ArgMatches) -> RS<()> {
    match matches.subcommand() {
        Some(("entity", sub_args)) => {
            let config = handle_entity_command(sub_args)?;
            run_gen_entity(&config)
        }
        Some(("message", sub_args)) => {
            let config = handle_message_command(sub_args)?;
            run_gen_message(&config)
        }
        Some((cmd, _)) => {
            Err(m_error!(EC::NoneErr, format!("unknow command: {}", cmd)))
        }
        None => {
            Err(m_error!(EC::NoneErr, "Provide a sub-command. Use --help to show help"))
        }
    }
}

fn handle_entity_command(sub_args: &ArgMatches) -> RS<EntityConfig> {
    let input: Vec<String> = sub_args
        .get_many::<String>("input-source-files")
        .ok_or_else(|| m_error!(EC::NoneErr, "no input source path specify"))?
        .map(|s| s.clone())
        .collect();

    let output = sub_args
        .get_one::<String>("output-source-folder")
        .ok_or_else(|| m_error!(EC::NoneErr, "no output path specified"))?
        .clone();

    let type_desc = sub_args.get_one::<String>("type-desc").cloned();

    let lang = sub_args
        .get_one::<String>("lang")
        .map(|s| s.clone())
        .unwrap_or_else(|| "rust".to_string());

    Ok(EntityConfig {
        input,
        output,
        type_desc,
        lang,
    })
}

fn handle_message_command(sub_args: &ArgMatches) -> RS<MessageConfig> {
    let input = sub_args
        .get_one::<String>("input-wit-file")
        .ok_or_else(|| m_error!(EC::NoneErr, "no input source file/directory specify"))?
        .clone();

    let output = sub_args
        .get_one::<String>("output-source-file")
        .ok_or_else(|| m_error!(EC::NoneErr, "no output path specified"))?
        .clone();

    let lang = sub_args
        .get_one::<String>("lang")
        .map(|s| s.clone())
        .unwrap_or_else(|| "rust".to_string());

    let namespace = sub_args.get_one::<String>("namespace")
        .cloned();

    Ok(MessageConfig {
        input,
        output,
        lang,
        namespace,
    })
}

fn main_inner<I, T>(args: I) -> RS<()>
where
    I: IntoIterator<Item=T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let matches = parse_arguments(args)?;
    process_arguments(matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_arguments_with_entity() {
        let args = vec![
            "mgen".to_string(),
            "entity".to_string(),
            "-i".to_string(),
            "test.sql".to_string(),
            "-o".to_string(),
            "output/".to_string(),
            "-t".to_string(),
            "types.json".to_string(),
            "-l".to_string(),
            "rust".to_string(),
        ];

        let matches = parse_arguments(args).unwrap();
        assert_eq!(matches.subcommand_name(), Some("entity"));
    }

    #[test]
    fn test_parse_arguments_with_message() {
        let args = vec![
            "mgen".to_string(),
            "message".to_string(),
            "-i".to_string(),
            "input.wit".to_string(),
            "-o".to_string(),
            "output.rs".to_string(),
            "-l".to_string(),
            "rust".to_string(),
        ];

        let matches = parse_arguments(args).unwrap();
        assert_eq!(matches.subcommand_name(), Some("message"));
    }

    #[test]
    fn test_parse_arguments_missing_required() {
        let args = vec![
            "mgen".to_string(),
            "entity".to_string(),
            // lost required argument
        ];

        let result = parse_arguments(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_process_arguments_no_subcommand() {
        use clap::Command;

        let cmd = Command::new("mgen");
        let matches = cmd.try_get_matches_from(vec!["mgen"]).unwrap();
        // lost sub-command
        let result = process_arguments(matches);
        assert!(result.is_err());
    }
}