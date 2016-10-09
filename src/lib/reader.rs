use std;

extern crate enum_derive;
extern crate nom;

use types;

custom_derive! {
    #[derive(Debug, IterVariants(LocalConfTypeVariants))]
    pub enum LocalConfType {
        SysLinux,  // syslinux/syslinux.cfg
        IsoLinux,  // isolinux/isolinux.cfg
        ExtLinux,  // extlinux/extlinux.conf
    }
}

impl LocalConfType {
    fn get_paths(self, root: std::path::PathBuf) -> Vec<std::path::PathBuf> {
        let gen_paths = |dir_name, file_name| {
            use std::iter::FromIterator;
            vec![
                root.clone().join(std::path::PathBuf::from_iter(
                    vec!["boot", dir_name, file_name])),
                root.clone().join(std::path::PathBuf::from_iter(
                    vec![dir_name, file_name])),
                root.join(std::path::PathBuf::from(file_name)),
            ]
        };

        match self {
            LocalConfType::SysLinux => gen_paths("syslinux", "syslinux.cfg"),
            LocalConfType::IsoLinux => gen_paths("isolinux", "isolinux.cfg"),
            LocalConfType::ExtLinux => gen_paths("extlinux", "extlinux.conf"),
        }
    }

    fn get_all_paths(root: std::path::PathBuf) -> Vec<std::path::PathBuf> {
        LocalConfType::iter_variants().map(
            |local_type| local_type.get_paths(root.clone())
        ).fold(
            Vec::new(),
            |mut all_paths, ref mut paths| {
                all_paths.append(paths);
                all_paths
            })
    }
}

#[derive(Debug)]
pub struct Reader {
    root_dir: std::path::PathBuf,
    conf_dir: std::path::PathBuf,
    conf_file_path: std::path::PathBuf,
}

// TODO: Detailed errors.
#[derive(Debug)]
pub struct ReaderError {}

impl std::convert::From<std::io::Error> for ReaderError {
    fn from(_: std::io::Error) -> ReaderError { ReaderError{} }
}

fn sanitize_path(path: std::path::PathBuf) -> std::path::PathBuf {
    use std::iter::FromIterator;
    // TODO: Support '..', but do not allow passing over the root dir.
    // TODO: Log warning on unsafe path component.
    std::path::PathBuf::from_iter(
        path.components().filter_map(
            |component| match component {
                std::path::Component::Prefix(_) => None,
                std::path::Component::CurDir    => None,
                std::path::Component::ParentDir => None,

                std::path::Component::Normal(v) => Some(v),
                std::path::Component::RootDir => {
                    Some(std::ffi::OsStr::new("/"))
                },
            }))
}

fn resolve_one_path(path: std::path::PathBuf,
                    root_dir: &std::path::PathBuf,
                    conf_dir: &std::path::PathBuf) -> std::path::PathBuf {
    let path = sanitize_path(path);
    match path.has_root() {
        // Relative to disk root.
        true => {
            use std::iter::FromIterator;
            let mut path = path.into_iter();
            path.next();   // Skip root component.
            root_dir.clone().join(std::path::PathBuf::from_iter(path))
        }

        // Relative to directory with configuration file.
        false => {
            conf_dir.clone().join(path)
        }
    }
}

trait PathResolver {
    fn resolve(self, root_dir: &std::path::PathBuf,
               conf_dir: &std::path::PathBuf) -> Self;
}

macro_rules! resolve_enum_paths {
    ($var: ident, $root_dir: ident, $conf_dir: ident,
            $($enum_variant: path), *) => {
        match $var {
            $(
                $enum_variant(path) => {
                    $enum_variant(resolve_one_path(path, $root_dir, $conf_dir))
                },
            )*
        }
    }
}

macro_rules! resolve_some_path {
    ($var: expr, $root_dir: ident, $conf_dir: ident) => {
        match $var {
            Some(path) => Some(resolve_one_path(path, $root_dir, $conf_dir)),
            None => None,
        }
    }
}

macro_rules! resolve_enum {
    ($var: ident, $root_dir: ident, $conf_dir: ident,
            $($enum_variant: path), *) => {
        match $var {
            $(
                $enum_variant(obj_with_path) => {
                    $enum_variant(obj_with_path.resolve($root_dir, $conf_dir))
                },
            )*
        }
    }
}

macro_rules! resolve_some {
    ($var: expr, $root_dir: ident, $conf_dir: ident) => {
        match $var {
            Some(obj_with_path) => {
                Some(obj_with_path.resolve($root_dir, $conf_dir))
            },

            None => None,
        }
    }
}

impl PathResolver for types::KernelFile {
    fn resolve(self, root_dir: &std::path::PathBuf,
               conf_dir: &std::path::PathBuf) -> types::KernelFile {
        resolve_enum_paths!(
            self, root_dir, conf_dir,
            types::KernelFile::Linux,
            types::KernelFile::Boot,
            types::KernelFile::BSS,
            types::KernelFile::PXE,
            types::KernelFile::FDImage,
            types::KernelFile::ComBoot,
            types::KernelFile::Com32)
    }
}

impl PathResolver for types::Kernel {
    fn resolve(mut self, root_dir: &std::path::PathBuf,
               conf_dir: &std::path::PathBuf) -> types::Kernel {
        self.kernel_file = resolve_some!(self.kernel_file, root_dir, conf_dir);
        self.initrd = resolve_some_path!(self.initrd, root_dir, conf_dir);
        self.fdt_dir = resolve_some_path!(self.fdt_dir, root_dir, conf_dir);
        self
    }
}

impl PathResolver for types::KernelOrConfig {
    fn resolve(self, root_dir: &std::path::PathBuf,
               conf_dir: &std::path::PathBuf) -> types::KernelOrConfig {
        resolve_enum!(
            self, root_dir, conf_dir,
            types::KernelOrConfig::Kernel)
    }
}

impl PathResolver for types::Label {
    fn resolve(mut self, root_dir: &std::path::PathBuf,
               conf_dir: &std::path::PathBuf) -> types::Label {
        self.kernel_or_config = self.kernel_or_config.resolve(
            root_dir, conf_dir);
        self.display = resolve_some_path!(self.display, root_dir, conf_dir);
        self
    }
}

impl PathResolver for types::Global {
    fn resolve(mut self, root_dir: &std::path::PathBuf,
               conf_dir: &std::path::PathBuf) -> types::Global {
        self.label_defaults = self.label_defaults.resolve(root_dir, conf_dir);
        self
    }
}

impl PathResolver for types::SyslinuxConf {
    fn resolve(mut self, root_dir: &std::path::PathBuf,
               conf_dir: &std::path::PathBuf) -> types::SyslinuxConf {
        self.global = self.global.resolve(root_dir, conf_dir);

        use std::iter::FromIterator;
        self.labels = types::Labels::from_iter(
            self.labels.into_iter().map(
                |(label_name, label)| {
                    (label_name, label.resolve(root_dir, conf_dir))
                }));

        self
    }
}

impl Reader {
    fn find_existing_local_conf(paths: Vec<std::path::PathBuf>)
            -> Result<std::path::PathBuf, ReaderError> {
        match paths.into_iter().find(|path| path.exists()) {
            Some(path) => Ok(path),
            None => Err(ReaderError{}),
        }
    }

    pub fn from_local_conf_file_path(root: std::path::PathBuf,
                                     conf_file_path: std::path::PathBuf)
            -> Result<Reader, ReaderError> {
        Ok(Reader{
            root_dir: root,
            conf_dir: match conf_file_path.parent() {
                Some(conf_dir) => conf_dir.to_path_buf(),
                None => return Err(ReaderError{}),
            },
            conf_file_path: conf_file_path,
        })
    }

    fn from_existing_local_conf(root: std::path::PathBuf,
                                paths: Vec<std::path::PathBuf>)
            -> Result<Reader, ReaderError> {
        Reader::from_local_conf_file_path(
            root,
            try!(Reader::find_existing_local_conf(paths)))
    }

    pub fn from_local_type(root: std::path::PathBuf, local_type: LocalConfType)
            -> Result<Reader, ReaderError> {
        Reader::from_existing_local_conf(
            root.clone(),
            local_type.get_paths(root))
    }

    pub fn from_local(root: std::path::PathBuf) -> Result<Reader, ReaderError> {
        Reader::from_existing_local_conf(
            root.clone(),
            LocalConfType::get_all_paths(root))
    }

    fn get_conf_contents(&self) -> Result<Vec<u8>, ReaderError> {
        let mut file = try!(std::fs::File::open(&self.conf_file_path));

        {
            use std::io::prelude::*;
            let mut buf = Vec::new();
            match file.read_to_end(&mut buf) {
                Ok(_) => Ok(buf),
                Err(_) => Err(ReaderError{}),
            }
        }
    }

    fn read_raw(&self) -> Result<types::SyslinuxConf, ReaderError> {
        match types::SyslinuxConf::parse(&try!(self.get_conf_contents())[..]) {
            nom::IResult::Done(remaining, conf) => match remaining.len() {
                0 => Ok(conf),
                _ => Err(ReaderError{}),
            },
            _ => Err(ReaderError{}),
        }
    }

    pub fn read(&self) -> Result<types::SyslinuxConf, ReaderError> {
        self.read_raw().map(
            |conf| conf.resolve(&self.root_dir, &self.conf_dir))
    }
}
