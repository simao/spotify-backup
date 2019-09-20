use std::env;
use spotify_backup::config::Config;

pub fn main() -> (){
    pretty_env_logger::init();

    let args: Vec<String> = env::args().collect();

    let config = Config::load().expect("could not load config");

    log::debug!("using config: {}", config);

    if args.len() == 2 && args[1] == "server" {
        spotify_backup::server::server(config).unwrap();
    } else if args.len() == 2 && args[1] == "daemon" {
        spotify_backup::server::daemon::daemon(config);
    }  else {
        spotify_backup::cli::cli();
    }
}
