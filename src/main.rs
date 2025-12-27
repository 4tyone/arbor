use arbor::cli::analyze::{run_analyze, AnalyzeArgs, OutputFormat};
use arbor::cli::database::{run_init, run_refresh, run_remove, run_export, InitOptions, ExportOptions};
use arbor::cli::query;
use arbor::core::config::ArborConfig;
use arbor::core::paths;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "arbor")]
#[command(about = "Static analysis tool for exception and None source extraction")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Analyze {
        #[arg(required_unless_present_any = ["all_public", "from_file"])]
        functions: Vec<String>,

        #[arg(short = 'd', long = "max-depth", default_value = "50")]
        depth: usize,

        #[arg(short, long, default_value = "markdown")]
        format: String,

        #[arg(long)]
        venv: Option<String>,

        #[arg(long)]
        all_public: Option<String>,

        #[arg(long)]
        from_file: Option<String>,
    },

    Query {
        #[command(subcommand)]
        query: QueryCommands,

        #[arg(short, long, default_value = "markdown", global = true)]
        format: String,
    },

    Init {
        #[arg(short, long)]
        force: bool,

        #[arg(long)]
        skip_site_packages: bool,
    },

    Refresh {
        functions: Vec<String>,
    },

    Remove {
        functions: Vec<String>,
    },

    Export {
        #[arg(short, long)]
        output: Option<String>,

        #[arg(short, long, default_value = "json")]
        format: String,
    },

    Config {
        #[command(subcommand)]
        config_cmd: ConfigCommands,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    Init {
        #[arg(short, long)]
        force: bool,
    },

    Show,

    Path,
}

#[derive(Subcommand)]
enum QueryCommands {
    Risk {
        function: String,
    },

    Has {
        function: String,
        exception: String,
    },

    Handle {
        function: String,
    },

    Signature {
        function: String,
    },

    OneException {
        function: String,
        exc_type: String,
    },

    OneNone {
        function: String,
        index: usize,
    },

    Callers {
        function: String,
    },

    Callees {
        function: String,
    },

    Diff {
        function: String,
    },

    Exceptions {
        function: String,
    },

    None {
        function: String,
    },

    Function {
        function: String,
    },

    Chain {
        function: String,
        exception: String,
    },

    Groups {
        package: Option<String>,
    },

    Exception {
        exc_type: String,
    },

    Package {
        name: String,
    },

    List,

    Search {
        query: String,
    },

    Stats,

    #[command(name = "quickref", visible_alias = "ref")]
    QuickRef,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Analyze { functions, depth, format, venv, all_public, from_file } => {
            let output_format = match format.as_str() {
                "json" => OutputFormat::Json,
                _ => OutputFormat::Markdown,
            };

            let mut all_functions = functions;

            if let Some(file_path) = from_file {
                match std::fs::read_to_string(&file_path) {
                    Ok(content) => {
                        for line in content.lines() {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                                all_functions.push(trimmed.to_string());
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error reading file {}: {}", file_path, e);
                        std::process::exit(1);
                    }
                }
            }

            if let Some(module_name) = all_public {
                // For now, just add the module as a function to analyze
                // The analyze command will discover public functions
                all_functions.push(format!("{}.*", module_name));
            }

            if all_functions.is_empty() {
                eprintln!("Error: No functions specified");
                std::process::exit(1);
            }

            let args = AnalyzeArgs {
                functions: all_functions,
                depth,
                output_format,
                venv_path: venv.map(std::path::PathBuf::from),
            };
            match run_analyze(args) {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Query { query: query_cmd, format } => {
            let use_json = format == "json";

            let result = match query_cmd {
                QueryCommands::Risk { function } => {
                    if use_json {
                        query::query_risk_json(&function)
                    } else {
                        query::query_risk(&function)
                    }
                }
                QueryCommands::Has { function, exception } => query::query_has(&function, &exception),
                QueryCommands::Handle { function } => query::query_handle(&function),
                QueryCommands::Signature { function } => query::query_signature(&function),
                QueryCommands::OneException { function, exc_type } => {
                    query::query_one_exception(&function, &exc_type)
                }
                QueryCommands::OneNone { function, index } => {
                    query::query_one_none(&function, index)
                }
                QueryCommands::Callers { function } => query::query_callers(&function),
                QueryCommands::Callees { function } => query::query_callees(&function),
                QueryCommands::Diff { function } => query::query_diff(&function),
                QueryCommands::Exceptions { function } => {
                    if use_json {
                        query::query_exceptions_json(&function)
                    } else {
                        query::query_exceptions(&function)
                    }
                }
                QueryCommands::None { function } => {
                    if use_json {
                        query::query_none_json(&function)
                    } else {
                        query::query_none(&function)
                    }
                }
                QueryCommands::Function { function } => {
                    if use_json {
                        query::query_function_json(&function)
                    } else {
                        query::query_function(&function)
                    }
                }
                QueryCommands::Chain { function, exception } => {
                    query::query_chain(&function, &exception)
                }
                QueryCommands::Groups { package } => {
                    if use_json {
                        query::query_groups_json(package.as_deref())
                    } else {
                        query::query_groups(package.as_deref())
                    }
                }
                QueryCommands::Exception { exc_type } => query::query_exception(&exc_type),
                QueryCommands::Package { name } => query::query_package(&name),
                QueryCommands::List => {
                    if use_json {
                        query::query_list_json()
                    } else {
                        query::query_list()
                    }
                }
                QueryCommands::Search { query: q } => query::query_search(&q),
                QueryCommands::Stats => {
                    if use_json {
                        query::query_stats_json()
                    } else {
                        query::query_stats()
                    }
                }
                QueryCommands::QuickRef => {
                    println!("{}", query::query_quickref());
                    return;
                }
            };

            match result {
                Ok(output) => println!("{}", output),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Init { force, skip_site_packages } => {
            let options = InitOptions {
                force,
                index_site_packages: !skip_site_packages,
            };
            match run_init(options) {
                Ok(path) => println!("\nDatabase ready: {}", path.display()),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Refresh { functions } => {
            match run_refresh(if functions.is_empty() { None } else { Some(functions) }) {
                Ok(count) => {
                    if count == 0 {
                        println!("\nNo functions refreshed (no changes detected)");
                    } else {
                        println!("\nRefreshed {} function(s)", count);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Remove { functions } => {
            if functions.is_empty() {
                match run_remove(None) {
                    Ok(()) => println!("\nDatabase removed"),
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                match run_remove(Some(functions.clone())) {
                    Ok(()) => println!("\nRemoved {} function(s) from database", functions.len()),
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        Commands::Export { output, format } => {
            let options = ExportOptions {
                output_path: output.map(std::path::PathBuf::from),
                format: format.clone(),
            };
            match run_export(options) {
                Ok(path) => println!("Exported to: {}", path.display()),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Config { config_cmd } => {
            match config_cmd {
                ConfigCommands::Init { force } => {
                    let config_path = paths::config_path();
                    if config_path.exists() && !force {
                        eprintln!("Config file already exists: {}", config_path.display());
                        eprintln!("Use --force to overwrite");
                        std::process::exit(1);
                    }
                    if let Err(e) = paths::ensure_arbor_dir() {
                        eprintln!("Error creating .arbor directory: {}", e);
                        std::process::exit(1);
                    }
                    let content = ArborConfig::default_toml();
                    match std::fs::write(&config_path, content) {
                        Ok(()) => println!("Created: {}", config_path.display()),
                        Err(e) => {
                            eprintln!("Error writing config: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                ConfigCommands::Show => {
                    let config = ArborConfig::load_or_default();
                    match toml::to_string_pretty(&config) {
                        Ok(s) => println!("{}", s),
                        Err(e) => {
                            eprintln!("Error serializing config: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                ConfigCommands::Path => {
                    match ArborConfig::find_config() {
                        Some(path) => println!("{}", path.display()),
                        None => println!("(no config file found, using defaults)"),
                    }
                }
            }
        }
    }
}

