#[macro_use] extern crate log;

extern crate clap;
extern crate env_logger;
extern crate nom;
extern crate serde_json;

extern crate syslinux_conf;

fn main() {
    env_logger::init().unwrap();

    let matches = clap::App::new("syslinux-tool")
        .about("Converts syslinux configuration file into JSON")
        .version(env!("CARGO_PKG_VERSION"))
        .arg(clap::Arg::with_name("type")
            .help("Type of syslinux configuration. Only for autodetect.")
            .short("t")
            .long("type")
            .value_name("TYPE")
            .takes_value(true)
            .possible_values(&["syslinux", "isolinux", "extlinux"]))
        .arg(clap::Arg::with_name("ROOT DIR")
            .help("Path to the root directory of the boot device.")
            .required(true)
            .index(1))
        .arg(clap::Arg::with_name("CONF FILE PATH")
            .help("Path to the configuration file. Will be autodetected if \
                   omitted.")
            .index(2))
        .group(clap::ArgGroup::with_name("detection")
            .arg("type")
            .arg("CONF FILE PATH"))
        .get_matches();

    let root_dir = matches.value_of("ROOT DIR").unwrap();
    let root_dir = std::path::PathBuf::from(root_dir);

    let reader = match matches.value_of("CONF FILE PATH") {
        Some(conf_path) => {
            let conf_path = std::path::PathBuf::from(conf_path);
            syslinux_conf::Reader::from_local_conf_file_path(
                root_dir, conf_path)
        }

        None => {
            match matches.value_of("type") {
                Some(conf_type) => {
                    let conf_type = match conf_type {
                        "syslinux" => syslinux_conf::LocalConfType::SysLinux,
                        "isolinux" => syslinux_conf::LocalConfType::IsoLinux,
                        "extlinux" => syslinux_conf::LocalConfType::ExtLinux,
                        _ => panic!("This will never happen"),
                    };
                    syslinux_conf::Reader::from_local_type(root_dir, conf_type)
                }

                None => {
                    syslinux_conf::Reader::from_local(root_dir)
                }
            }
        }
    };

    let reader = match reader {
        Ok(reader) => reader,
        Err(_) => {
            // TODO: Log actual reason.
            error!("Unable to create syslinux configuration reader");
            std::process::exit(1)
        },
    };

    let data = match reader.read() {
        Ok(data) => data,
        Err(_) => {
            // TODO: Log actual reason.
            error!("Unable to read syslinux configuration");
            std::process::exit(1)
        },
    };

    let json = match serde_json::to_string(&data) {
        Ok(json) => json,
        Err(_) => {
            error!("Unable to serialize syslinux configuration as JSON");
            std::process::exit(1)
        },
    };

    use std::io::Write;
    match std::io::stdout().write(&json.into_bytes()[..]) {
        Ok(_) => (),
        Err(_) => {
            error!("Unable to write JSON to stdout");
            std::process::exit(1)
        },
    };
}
