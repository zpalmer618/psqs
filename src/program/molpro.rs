use std::fs::File;

use regex::Regex;

use crate::geom::{geom_string, Geom};

use super::{Procedure, Program, Template};

#[cfg(test)]
mod tests;

pub struct Molpro {
    filename: String,
    template: Template,
    charge: isize,
    geom: Geom,
}

impl Molpro {
    pub fn new(
        filename: String,
        template: Template,
        charge: isize,
        geom: Geom,
    ) -> Self {
        Self {
            filename,
            template,
            charge,
            geom,
        }
    }
}

impl Program for Molpro {
    fn filename(&self) -> String {
        self.filename.clone()
    }

    fn set_filename(&mut self, filename: &str) {
        self.filename = String::from(filename);
    }

    fn template(&self) -> &Template {
        &self.template
    }

    fn extension(&self) -> String {
        String::from("inp")
    }

    fn charge(&self) -> isize {
        self.charge
    }

    /// Example [Template]:
    /// ```text
    /// memory,1,g
    /// gthresh,energy=1.d-12,zero=1.d-22,oneint=1.d-22,twoint=1.d-22;
    /// gthresh,optgrad=1.d-8,optstep=1.d-8;
    /// nocompress;
    ///
    /// geometry={
    /// {{.geom}}
    /// ! note the missing closing brace!
    /// basis={
    /// default,cc-pVTZ-f12
    /// }
    /// set,charge={{.charge}}
    /// set,spin=0
    /// hf,accuracy=16,energy=1.0d-10
    /// {CCSD(T)-F12,thrden=1.0d-8,thrvar=1.0d-10}
    /// {optg,grms=1.d-8,srms=1.d-8}
    /// ```
    ///
    /// In line with [Go templates](https://pkg.go.dev/text/template),
    /// `{{.geom}}` is replaced with `self.geom`, and `{{.charge}}` is
    /// replaced with `self.charge`. If `proc` is `Procedure::Opt`, and the
    /// template includes this optg line, the line is left there. If the
    /// procedure is `Opt` and the line is absent, it will be added.
    /// Similarly, if `proc` is not `Opt` and the line is present in the
    /// template, it will be deleted.
    ///
    /// The missing closing brace around the geometry allows for easier handling
    /// of ZMAT inputs since `write_input` can insert its own closing brace
    /// between the ZMAT and parameter values.
    fn write_input(&mut self, proc: Procedure) {
        use std::io::Write;
        let mut body = self.template().clone().header;
        // skip optgrad but accept optg at the end of a line
        lazy_static::lazy_static! {
        static ref OPTG: Regex = Regex::new(r"(?i)optg(,|\s*$)").unwrap();
        static ref OPTG_LINE: Regex = Regex::new(r"(?i)^.*optg(,|\s*$)").unwrap();
        static ref CHARGE: Regex = Regex::new(r"\{\{.charge\}\}").unwrap();
        static ref GEOM: Regex = Regex::new(r"\{\{.geom\}\}").unwrap();
        }
        let mut found_opt = false;
        if OPTG.is_match(&body) {
            found_opt = true;
        }
        {
            use std::fmt::Write;
            match proc {
                Procedure::Opt => {
                    if !found_opt {
                        writeln!(body, "{{optg,grms=1.d-8,srms=1.d-8}}")
                            .unwrap();
                    }
                }
                Procedure::Freq => todo!(),
                Procedure::SinglePt => {
                    if found_opt {
                        let mut new = String::new();
                        for line in body.lines() {
                            if !OPTG_LINE.is_match(line) {
                                writeln!(new, "{line}").unwrap();
                            }
                        }
                        body = new;
                    }
                }
            }
        }
        let geom = geom_string(&self.geom);
        let geom = if let Geom::Zmat(_) = &self.geom {
            use std::fmt::Write;
            let mut new_lines = String::new();
            let mut found = false;
            for line in geom.lines() {
                if line.contains('=') && !found {
                    found = true;
                    new_lines.push_str("}\n");
                }
                writeln!(new_lines, "{line}").unwrap();
            }
            new_lines
        } else {
            geom
        };
        body = GEOM.replace(&body, geom).to_string();
        body = CHARGE
            .replace(&body, &format!("{}", self.charge))
            .to_string();

        let filename = format!("{}.{}", self.filename, self.extension());
        let mut file = match File::create(&filename) {
            Ok(f) => f,
            Err(e) => panic!("failed to create {filename} with {e}"),
        };
        write!(file, "{}", body).expect("failed to write input file");
    }

    fn read_output(&self) -> Result<super::ProgramResult, super::ProgramError> {
        todo!()
    }

    fn associated_files(&self) -> Vec<String> {
        let fname = self.filename();
        vec![format!("{}.inp", fname), format!("{}.out", fname)]
    }
}
