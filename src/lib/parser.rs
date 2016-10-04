use std;

extern crate nom;

use types::{KernelFile, KernelOrConfig, Label, Global, Labels, SyslinuxConf};

// TODO: Support INCLUDE tag.
// TODO: Support CONFIG tags.

enum Error {
    FromUTF8Failed,
    InvalidTag,
    NextLabelFound,
}

macro_rules! some2 {
    ($expr: expr) => { Some(Some($expr)) }
}

named!(
    skip_spaces0(&[u8]) -> (),
    fold_many0!(
        nom::space,
        (),
        |_, _| ()));

named!(
    skip_spaces1(&[u8]) -> (),
    fold_many1!(
        nom::space,
        (),
        |_, _| ()));

named!(skip_line_end,
       alt_complete!(tag!("\n") | tag!("\r") | tag!("\r\n") | call!(nom::eof)));

fn skip_tag_ci<'a>(input: &'a [u8],
                   tag_upper_case: &str)
                   -> nom::IResult<&'a [u8], ()> {
    let match_tag = |input_part, tag_part, good_result| {
        use std::ascii::AsciiExt;
        match std::str::from_utf8(input_part) {
            Result::Ok(s) => {
                match s.to_ascii_uppercase() == tag_part {
                    true => good_result,
                    false => nom::IResult::Error(nom::Err::Position(
                    nom::ErrorKind::Custom(Error::FromUTF8Failed as u32),
                    input)),
                }
            }

            Result::Err(_) => nom::IResult::Error(nom::Err::Position(
                nom::ErrorKind::Custom(Error::InvalidTag as u32),
                input)),
        }
    };

    match (input.len(), tag_upper_case.len()) {
        // We have enough data to compare full tag name.
        (input_len, tag_len) if input_len >= tag_len => {
            match_tag(&input[..tag_len],
                      tag_upper_case,
                      nom::IResult::Done(&input[tag_len..], ()))
        }

        // Not enough data => try to compare what we have.
        (input_len, tag_len) => {
            match_tag(input,
                      &tag_upper_case[..input_len],
                      nom::IResult::Incomplete(nom::Needed::Size(tag_len)))
        }
    }
}

named!(
    get_line(&[u8]) -> String,
    chain!(
        line: map!(
            map_res!(
                is_not!("\r\n"),   // Will take chars until '\r', '\n' or EOF.
                std::str::from_utf8),
            |v: &str| String::from(v.trim())) ~
         skip_line_end,
         || line));

named!(
    get_u32(&[u8]) -> u32,
    map_res!(
        get_line,
        |s: String| s.parse::<u32>()));

named!(
    get_path(&[u8]) -> std::path::PathBuf,
    map!(
        get_line,
        |s: String| std::path::PathBuf::from(s)));

named!(
    skip_empty_line(&[u8]) -> (),
    chain!(
        skip_spaces0 ~
        skip_line_end,
        || ()));

named!(
    get_comment_line(&[u8]) -> String,
    chain!(
        skip_spaces0 ~
        tag!("#") ~
        line: get_line,
        || line));

named!(
    skip_insignificant(&[u8]) -> (),
    alt_complete!(
        skip_empty_line  => { |_| () } |
        get_comment_line => { |_| () }));

macro_rules! named_tag_ci {
    ($name: ident, $tag: expr, $result_type: ty, $parser: ident) => {
        named!(
            $name(&[u8]) -> $result_type,
            chain!(
                skip_spaces0 ~
                call!(skip_tag_ci, $tag) ~
                skip_spaces1 ~
                result: $parser,
                || result));
    }
}

trait LineParser<FieldType> {
    fn parse_line(&[u8]) -> nom::IResult<&[u8], Option<Option<FieldType>>>;

    fn get_value<'a>(input: &'a [u8])
                     -> nom::IResult<&'a [u8], Option<FieldType>>
        where FieldType: LineParser<FieldType>
    {
        match FieldType::parse_line(input) {
            // Next LABEL found => stop parsing.
            nom::IResult::Done(i, None) => {
                nom::IResult::Error(nom::Err::Position(
                    nom::ErrorKind::Custom(Error::NextLabelFound as u32), i))
            }

            // Normal tag value or empty/comment line or unknown/unsupported
            // tag.
            nom::IResult::Done(i, Some(v)) => nom::IResult::Done(i, v),

            nom::IResult::Incomplete(v) => nom::IResult::Incomplete(v),
            nom::IResult::Error(v) => nom::IResult::Error(v),
        }
    }
}

trait StructBuilder<FieldType, StructType> {
    fn build(mut self, f: FieldType) -> StructType;

    fn parse(input: &[u8]) -> nom::IResult<&[u8], StructType>
        where FieldType: LineParser<FieldType>,
              StructType: Default + StructBuilder<FieldType, StructType>
    {

        let build_or_skip = |s, f| {
            match f {
                Some(v) => StructType::build(s, v),
                // Empty line or comment line or unknown/unsupported tag.
                None => s,
            }
        };

        fold_many0!(input,
                    FieldType::get_value,
                    StructType::default(),
                    build_or_skip)
    }
}

fn get_extension(file_path: &std::path::PathBuf) -> &str {
    match file_path.extension() {
        Some(extension) => match extension.to_str() {
            Some(extension) => extension,
            None => {
                warn!("Unable to decode extension of file \"{:?}\"", file_path);
                ""
            },
        },
        None => "",
    }
}

impl KernelFile {
    fn guess(kernel_path: std::path::PathBuf) -> KernelFile {
        match get_extension(&kernel_path) {
            "bin" | "bs"  => KernelFile::Boot(kernel_path),
            "bss"         => KernelFile::BSS(kernel_path),
            "0"           => KernelFile::PXE(kernel_path),
            "img"         => KernelFile::FDImage(kernel_path),
            "cbt" | "com" => KernelFile::ComBoot(kernel_path),
            "c32"         => KernelFile::Com32(kernel_path),
            _             => KernelFile::Linux(kernel_path),
        }
    }
}

#[derive(Debug)]
enum LabelKernelOrConfigField {
    KernelFile(KernelFile),
    InitRD(std::path::PathBuf),
    FDTDir(std::path::PathBuf),
    Append(String),
}

#[derive(Debug)]
enum LabelField {
    TextHelp(String),
    KernelOrConfig(LabelKernelOrConfigField),
    Say(String),
    Display(std::path::PathBuf),
}

macro_rules! catch_label_field {
    ($enum_id: ident) => { |v| some2!(LabelField::$enum_id(v)) }
}

macro_rules! catch_kernel_field {
    ($kernel_field: ident) => {
        |v| some2!(
                LabelField::KernelOrConfig(
                    LabelKernelOrConfigField::$kernel_field(v)))
    }
}

macro_rules! catch_kernel_file_type {
    ($kernel_file_type: ident) => {
        |v| some2!(
                LabelField::KernelOrConfig(
                    LabelKernelOrConfigField::KernelFile(
                        KernelFile::$kernel_file_type(v))))
    }
}

// Tag that starts LABEL scope.
named_tag_ci!(get_tag_label, "LABEL", String, get_line);

// Tags valid in LABEL scope.
named_tag_ci!(get_tag_kernel,  "KERNEL",  std::path::PathBuf, get_path);
named_tag_ci!(get_tag_linux,   "LINUX",   std::path::PathBuf, get_path);
named_tag_ci!(get_tag_boot,    "BOOT",    std::path::PathBuf, get_path);
named_tag_ci!(get_tag_bss,     "BSS",     std::path::PathBuf, get_path);
named_tag_ci!(get_tag_pxe,     "PXE",     std::path::PathBuf, get_path);
named_tag_ci!(get_tag_fdimage, "FDIMAGE", std::path::PathBuf, get_path);
named_tag_ci!(get_tag_comboot, "COMBOOT", std::path::PathBuf, get_path);
named_tag_ci!(get_tag_com32,   "COM32",   std::path::PathBuf, get_path);
named_tag_ci!(get_tag_initrd,  "INITRD",  std::path::PathBuf, get_path);
named_tag_ci!(get_tag_fdtdir,  "FDTDIR",  std::path::PathBuf, get_path);
named_tag_ci!(get_tag_append,  "APPEND",  String,             get_line);
named_tag_ci!(get_tag_say,     "SAY",     String,             get_line);
named_tag_ci!(get_tag_display, "DISPLAY", std::path::PathBuf, get_path);
named!(
    get_tag_text_help(&[u8]) -> String,
    chain!(
        call!(skip_tag_ci, "TEXT") ~
        skip_spaces1 ~
        call!(skip_tag_ci, "HELP") ~
        skip_spaces0 ~
        skip_line_end ~
        text: fold_many0!(
            map_res!(
                get_line,
                |line: String| {
                    use std::ascii::AsciiExt;
                    if line.to_ascii_uppercase() == "ENDTEXT" {
                        Err(())
                    } else {
                        Ok(line)
                    }}),
            String::new(),
            |acc: String, item: String| {
                use std::ops::Add;
                match acc.is_empty() {
                    true => acc.add(&item),
                    false => acc.add(" ").add(&item),
                }
            }) ~
        call!(skip_tag_ci, "ENDTEXT"),
        || text));

impl LabelField {
    named!(
        parse_tag(&[u8]) -> Option<Option<LabelField> >,
        alt_complete!(
            // Next LABEL found.
            peek!(get_tag_label) => { |_| None } |
            // Empty line or comment line.
            skip_insignificant => { |_| Some(None) } |

            // All tags that are valid in LABEL scope.
            get_tag_text_help => {      catch_label_field!(TextHelp) } |
            get_tag_say       => {      catch_label_field!(Say)      } |
            get_tag_display   => {      catch_label_field!(Display)  } |
            get_tag_linux     => { catch_kernel_file_type!(Linux)    } |
            get_tag_boot      => { catch_kernel_file_type!(Boot)     } |
            get_tag_bss       => { catch_kernel_file_type!(BSS)      } |
            get_tag_pxe       => { catch_kernel_file_type!(PXE)      } |
            get_tag_fdimage   => { catch_kernel_file_type!(FDImage)  } |
            get_tag_comboot   => { catch_kernel_file_type!(ComBoot)  } |
            get_tag_com32     => { catch_kernel_file_type!(Com32)    } |
            get_tag_initrd    => {     catch_kernel_field!(InitRD)   } |
            get_tag_fdtdir    => {     catch_kernel_field!(FDTDir)   } |
            get_tag_append    => {     catch_kernel_field!(Append)   } |
            get_tag_kernel    => {
                |v| some2!(
                        LabelField::KernelOrConfig(
                            LabelKernelOrConfigField::KernelFile(
                                KernelFile::guess(v))))
            }));
}

impl LineParser<LabelField> for LabelField {
    named!(
        parse_line(&[u8]) -> Option<Option<LabelField> >,
        alt!(
            // Try to parse known tags.
            call!(LabelField::parse_tag) |
            // No known tags => unknown/unsupported tag.
            get_line => { |line| {
                debug!("Unknown or unsupported tag: \"{}\"", line);
                Some(None)
            }}));
}

impl Label {
    fn build_kernel(&mut self, field: LabelKernelOrConfigField) {
        match self.kernel_or_config {
            KernelOrConfig::Kernel(ref mut k) => match field {
                LabelKernelOrConfigField::KernelFile(v) =>
                    k.kernel_file = Some(v),
                LabelKernelOrConfigField::InitRD(v)     =>
                    k.initrd      = Some(v),
                LabelKernelOrConfigField::FDTDir(v)     =>
                    k.fdt_dir     = Some(v),
                LabelKernelOrConfigField::Append(v)     =>
                    k.append      = Some(v),
            },
        }
    }
}

impl StructBuilder<LabelField, Label> for Label {
    fn build(mut self, field: LabelField) -> Label {
        match field {
            LabelField::TextHelp(v)       => self.text_help = Some(v),
            LabelField::Say(v)            => self.say       = Some(v),
            LabelField::Display(v)        => self.display   = Some(v),
            LabelField::KernelOrConfig(v) => self.build_kernel(v),
        };
        self
    }
}

#[derive(Debug)]
enum GlobalField {
    Default(String),
    OnTimeout(String),
    OnError(String),
    Timeout(u32),
    TotalTimeout(u32),
    Label(LabelField),
}

// Tags valid in global scope.
named_tag_ci!(get_tag_default,      "DEFAULT",      String, get_line);
named_tag_ci!(get_tag_ontimeout,    "ONTIMEOUT",    String, get_line);
named_tag_ci!(get_tag_onerror,      "ONERROR",      String, get_line);
named_tag_ci!(get_tag_timeout,      "TIMEOUT",      u32,    get_u32);
named_tag_ci!(get_tag_totaltimeout, "TOTALTIMEOUT", u32,    get_u32);

impl GlobalField {
    named!(
        get_tag(&[u8]) -> GlobalField,
        alt_complete!(
            get_tag_default      => { |v| GlobalField::Default(v)      } |
            get_tag_ontimeout    => { |v| GlobalField::OnTimeout(v)    } |
            get_tag_onerror      => { |v| GlobalField::OnError(v)      } |
            get_tag_timeout      => { |v| GlobalField::Timeout(v)      } |
            get_tag_totaltimeout => { |v| GlobalField::TotalTimeout(v) }));
}

impl LineParser<GlobalField> for GlobalField {
    named!(
        parse_line(&[u8]) -> Option<Option<GlobalField> >,
        alt!(
            call!(GlobalField::get_tag) => {
                |v| some2!(v)
            } |

            call!(LabelField::parse_line) => {
                |v| match v {
                    Some(Some(label_field)) => {
                        some2!(GlobalField::Label(label_field))
                    },
                    Some(None) => Some(None),
                    None => None,
                }
            }));
}

impl Global {
    fn conv_timeout(timeout: u32) -> Option<f64> {
        match timeout {
            // Zero means disabled timeout.
            0 => None,
            // Timeout is in units of 1/10s.
            t => Some(t as f64 / 10.0),
        }
    }
}

impl StructBuilder<GlobalField, Global> for Global {
    fn build(mut self, field: GlobalField) -> Global {
        match field {
            GlobalField::Default(v)   => self.default   = Some(v),
            GlobalField::OnTimeout(v) => self.ontimeout = Some(v),
            GlobalField::OnError(v)   => self.onerror   = Some(v),

            GlobalField::Timeout(v)      => self.timeout =
                Global::conv_timeout(v),
            GlobalField::TotalTimeout(v) => self.total_timeout =
                Global::conv_timeout(v),

            GlobalField::Label(v) => {
                self.label_defaults = Label::build(self.label_defaults, v)
            }
        };
        self
    }
}

trait LabelBuilder {
    fn build(mut self, label_pair: (String, Label)) -> Labels;
}

impl LabelBuilder for Labels {
    fn build(mut self, label_pair: (String, Label)) -> Labels {
        let (label_name, label_data) = label_pair;

        match self.get(&label_name) {
            // Label with this name already exists => ignore it.
            Some(_) => {
                warn!("Duplicate label \"{}\"", label_name);
                self
            },
            None => {
                self.insert(label_name, label_data);
                self
            }
        }
    }
}

impl SyslinuxConf {
    named!(
        pub parse(&[u8]) -> SyslinuxConf,
        chain!(
            global: call!(Global::parse) ~
            labels: fold_many0!(
                chain!(
                    label_name: get_tag_label ~
                    label_data: call!(Label::parse),
                    || (label_name, label_data) ),
                Labels::new(),
                Labels::build),
            move || SyslinuxConf{
                    global: global,
                    labels: labels,
            }
        )
    );
}
