mod test_tool;
mod test_tool_wit_schema;

use clap::{Arg, ArgAction, ArgMatches, Command};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_gen::src_check::check_driver::{CheckOutcome, run_check_sql};
use mudu_gen::src_gen::codegen_cfg::CodegenCfg;
use mudu_gen::src_gen::gen_entity::gen_rust;
use mudu_gen::src_gen::gen_message::gen_message_with_cfg;
use sql_parser::check::diagnostic::SqlSeverity;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let r = main_inner(mudu_sys::env_var::args_os());
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
    pub namespace: Option<String>,
    pub with_func_codec: bool,
    pub type_desc: Option<String>,
}

#[derive(Debug)]
struct CheckSqlConfig {
    pub lang: String,
    pub ddl: Vec<String>,
    pub input: String,
    pub verbose: bool,
}

fn run_check_sql_command(config: &CheckSqlConfig) -> RS<()> {
    let outcome: CheckOutcome = run_check_sql(&config.lang, &config.ddl, &config.input)?;
    for diagnostic in &outcome.diagnostics {
        if diagnostic.severity == SqlSeverity::Error || config.verbose {
            println!("{}", diagnostic.rendered);
        }
    }
    if config.verbose {
        println!(
            "check-sql: {} file(s), {} SQL literal(s), {} error(s), {} uncovered",
            outcome.file_count, outcome.literal_count, outcome.error_count, outcome.uncovered_count
        );
    }
    if outcome.error_count > 0 {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            format!(
                "check-sql found {} error diagnostic(s)",
                outcome.error_count
            )
        ));
    }
    Ok(())
}

fn run_gen_message(config: &MessageConfig) -> RS<()> {
    let mut cfg = CodegenCfg::new();
    cfg.with_func_codec = config.with_func_codec;
    gen_message_with_cfg(
        &config.input,
        config.output.clone(),
        config.lang.clone(),
        config.namespace.clone(),
        cfg,
        config.type_desc.clone(),
    )?;
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
    I: IntoIterator<Item = T>,
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
                )
                .arg(
                    Arg::new("with-func-codec")
                        .long("with-func-codec")
                        .action(ArgAction::SetTrue)
                        .required(false)
                        .global(false)
                        .help("Generate MSSP syscall frame/func codecs for WIT func items"),
                )
                .arg(
                    Arg::new("type-desc")
                        .short('t')
                        .long("type-desc")
                        .value_name("FILE")
                        .required(false)
                        .global(false)
                        .help("Output syscall schema description JSON file path"),
                ),
        )
        .subcommand(
            Command::new("check-sql")
                .about(
                    "Statically check SQL string literals in guest sources against the DDL schema",
                )
                .arg(
                    Arg::new("lang")
                        .short('l')
                        .long("lang")
                        .value_name("LANG")
                        .required(true)
                        .help("Source language: rust | as | cs | py | c | go"),
                )
                .arg(
                    Arg::new("ddl")
                        .short('d')
                        .long("ddl")
                        .value_name("FILE")
                        .required(true)
                        .num_args(1..)
                        .help("DDL SQL file(s) providing the schema"),
                )
                .arg(
                    Arg::new("input")
                        .short('i')
                        .long("input")
                        .value_name("PATH")
                        .required(true)
                        .help("Source file or directory to check"),
                ),
        );
    let r_matches = command.try_get_matches_from(args);
    let matches = match r_matches {
        Ok(matches) => matches,
        Err(e) => {
            if e.kind() == clap::error::ErrorKind::DisplayHelp
                || e.kind() == clap::error::ErrorKind::DisplayVersion
            {
                eprintln!("{}", e);
                mudu_sys::process::exit(0);
            } else {
                eprintln!("parse arguments error: \n{}", e);
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!("parse arguments error: {}", e)
                ));
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
        Some(("check-sql", sub_args)) => {
            let config = handle_check_sql_command(sub_args)?;
            run_check_sql_command(&config)
        }
        Some((cmd, _)) => Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("unknow command: {}", cmd)
        )),
        None => Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "Provide a sub-command. Use --help to show help"
        )),
    }
}

fn handle_entity_command(sub_args: &ArgMatches) -> RS<EntityConfig> {
    let input: Vec<String> = sub_args
        .get_many::<String>("input-source-files")
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "no input source path specify"))?
        .cloned()
        .collect();

    let output = sub_args
        .get_one::<String>("output-source-folder")
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "no output path specified"))?
        .clone();

    let type_desc = sub_args.get_one::<String>("type-desc").cloned();

    let lang = sub_args
        .get_one::<String>("lang")
        .cloned()
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
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidArgument,
                "no input source file/directory specify"
            )
        })?
        .clone();

    let output = sub_args
        .get_one::<String>("output-source-file")
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "no output path specified"))?
        .clone();

    let lang = sub_args
        .get_one::<String>("lang")
        .cloned()
        .unwrap_or_else(|| "rust".to_string());

    let namespace = sub_args.get_one::<String>("namespace").cloned();

    let with_func_codec = sub_args.get_flag("with-func-codec");

    let type_desc = sub_args.get_one::<String>("type-desc").cloned();

    Ok(MessageConfig {
        input,
        output,
        lang,
        namespace,
        with_func_codec,
        type_desc,
    })
}

fn handle_check_sql_command(sub_args: &ArgMatches) -> RS<CheckSqlConfig> {
    let lang = sub_args
        .get_one::<String>("lang")
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "no lang specified"))?
        .clone();

    let ddl: Vec<String> = sub_args
        .get_many::<String>("ddl")
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "no DDL file specified"))?
        .cloned()
        .collect();

    let input = sub_args
        .get_one::<String>("input")
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "no input path specified"))?
        .clone();

    let verbose = sub_args.get_flag("verbose");

    Ok(CheckSqlConfig {
        lang,
        ddl,
        input,
        verbose,
    })
}

fn main_inner<I, T>(args: I) -> RS<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let matches = parse_arguments(args)?;
    process_arguments(matches)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
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

    fn fixture(name: &str) -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/src_check/fixtures")
            .join(name)
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn test_check_sql_clean_sources_exit_ok() {
        let args = vec![
            "mgen".to_string(),
            "check-sql".to_string(),
            "-l".to_string(),
            "rust".to_string(),
            "-d".to_string(),
            fixture("check_schema.sql"),
            "-i".to_string(),
            fixture("good.rs"),
        ];
        main_inner(args).unwrap();
    }

    #[test]
    fn test_check_sql_bad_sql_exit_err() {
        let args = vec![
            "mgen".to_string(),
            "check-sql".to_string(),
            "-l".to_string(),
            "rust".to_string(),
            "-d".to_string(),
            fixture("check_schema.sql"),
            "-i".to_string(),
            fixture("bad_table.rs"),
        ];
        // Any Error diagnostic makes the command fail (process exit code 1).
        assert!(main_inner(args).is_err());
    }
}
