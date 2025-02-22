use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::Duration;
use std::{collections::HashSet, process::Command};

use serde::{Deserialize, Serialize};

use crate::program::molpro::Molpro;
use crate::program::mopac::Mopac;
use crate::program::Program;
use crate::queue::Queue;

use super::{SubQueue, Submit};

/// Pbs is a type for holding the information for submitting a pbs job.
/// `filename` is the name of the Pbs submission script
#[derive(Debug)]
pub struct Pbs {
    chunk_size: usize,
    job_limit: usize,
    sleep_int: usize,
    dir: &'static str,
    no_del: bool,
    template: Option<String>,
}

impl Pbs {
    pub fn new(
        chunk_size: usize,
        job_limit: usize,
        sleep_int: usize,
        dir: &'static str,
        no_del: bool,
        template: Option<String>,
    ) -> Self {
        Self {
            chunk_size,
            job_limit,
            sleep_int,
            dir,
            no_del,
            template,
        }
    }
}

impl Submit<Mopac> for Pbs
where
    Mopac: Serialize + for<'a> Deserialize<'a>,
{
    /// submit `filename` to the queue and return the jobid
    fn submit(&self, filename: &str) -> String {
        let mut cmd =
            Command::new(<Self as SubQueue<Mopac>>::submit_command(self));
        let cmd = cmd.arg("-f").arg(filename);
        submit_inner(cmd, self.sleep_int).unwrap()
    }
}

// Molpro 2022 submit script requires submission from the current directory, so
// we have to override the default impl
impl Submit<Molpro> for Pbs
where
    Molpro: Serialize + for<'a> Deserialize<'a>,
{
    fn submit(&self, filename: &str) -> String {
        let path = Path::new(filename);
        let dir = path.parent().unwrap();
        let base = path.file_name().unwrap();
        let mut cmd =
            Command::new(<Self as SubQueue<Molpro>>::submit_command(self));
        let cmd = cmd.arg(base).current_dir(dir);
        submit_inner(cmd, self.sleep_int).unwrap()
    }
}

/// helper function to consolidate error handling between the two submit
/// implementations
fn submit_inner(
    cmd: &mut Command,
    sleep_int: usize,
) -> std::io::Result<String> {
    let mut retries = 5;
    loop {
        match cmd.output() {
            Ok(s) => {
                if !s.status.success() {
                    if retries > 0 {
                        eprintln!(
                            "qsub failed with output: {s:#?}, \
				   retrying {retries} more times"
                        );
                        retries -= 1;
                        std::thread::sleep(Duration::from_secs(
                            sleep_int as u64,
                        ));
                        continue;
                    }
                    panic!("qsub failed with output: {s:#?}");
                }
                let raw =
                    std::str::from_utf8(&s.stdout).unwrap().trim().to_string();
                return Ok(raw
                    .split_whitespace()
                    .last()
                    .unwrap_or("no jobid")
                    .to_string());
            }
            Err(e) => return Err(e),
        }
    }
}

impl Queue<Molpro> for Pbs
where
    Molpro: Serialize + for<'a> Deserialize<'a>,
{
    /// An example of `self.template` should look like
    ///
    fn write_submit_script(&self, infiles: &[String], filename: &str) {
        let path = Path::new(filename);
        let basename = path.file_name().unwrap();
        let mut body = self
            .template
            .clone()
            .unwrap_or_else(|| {
                <Self as Queue<Molpro>>::default_submit_script(self)
            })
            .replace("{{.basename}}", basename.to_str().unwrap());
        {
            use std::fmt::Write;
            for f in infiles {
                let basename = Path::new(f).file_name().unwrap();
                writeln!(
                    body,
                    "molpro -t $NCPUS --no-xml-output {basename:?}.inp"
                )
                .unwrap();
            }
            writeln!(body, "rm -rf $TMPDIR").unwrap();
        }
        let mut file = match File::create(filename) {
            Ok(f) => f,
            Err(_) => {
                panic!("write_submit_script: failed to create {filename}");
            }
        };
        write!(file, "{body}").unwrap_or_else(|_| {
            panic!("failed to write molpro input file: {filename}")
        });
    }

    fn default_submit_script(&self) -> String {
        "#!/bin/sh
#PBS -N {{.basename}}
#PBS -S /bin/bash
#PBS -j oe
#PBS -o {{.basename}}.out
#PBS -W umask=022
#PBS -l walltime=1000:00:00
#PBS -l ncpus=1
#PBS -l mem=8gb
#PBS -q workq

module load openpbs molpro

export WORKDIR=$PBS_O_WORKDIR
export TMPDIR=/tmp/$USER/$PBS_JOBID
cd $WORKDIR
mkdir -p $TMPDIR
"
        .to_owned()
    }
}

impl Queue<Mopac> for Pbs {
    /// An example of `self.template` should look like
    ///
    fn write_submit_script(&self, infiles: &[String], filename: &str) {
        let path = Path::new(filename);
        let basename = path.file_name().unwrap();
        let mut body = self
            .template
            .clone()
            .unwrap_or_else(|| {
                <Self as Queue<Mopac>>::default_submit_script(self)
            })
            .replace("{{.basename}}", basename.to_str().unwrap())
            .replace("{{.filename}}", filename);
        for f in infiles {
            body.push_str(&format!(
                "/ddn/home1/r2518/Packages/mopac/build/mopac {f}.mop\n"
            ));
        }
        let mut file = match File::create(filename) {
            Ok(f) => f,
            Err(_) => {
                eprintln!("write_submit_script: failed to create {filename}");
                std::process::exit(1);
            }
        };
        write!(file, "{body}").expect("failed to write params file");
    }

    fn default_submit_script(&self) -> String {
        "#!/bin/sh
#PBS -N {{.basename}}
#PBS -S /bin/bash
#PBS -j oe
#PBS -o {{.filename}}.out
#PBS -W umask=022
#PBS -l walltime=1000:00:00
#PBS -l ncpus=1
#PBS -l mem=1gb
#PBS -q workq

module load openpbs

export WORKDIR=$PBS_O_WORKDIR
cd $WORKDIR

"
        .to_owned()
    }
}

impl<P> SubQueue<P> for Pbs
where
    P: Program + Clone + Serialize + for<'a> Deserialize<'a>,
{
    fn submit_command(&self) -> &str {
        "qsub"
    }

    fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    fn job_limit(&self) -> usize {
        self.job_limit
    }

    fn sleep_int(&self) -> usize {
        self.sleep_int
    }

    const SCRIPT_EXT: &'static str = "pbs";

    fn dir(&self) -> &str {
        self.dir
    }

    /// run `qstat -u $USER`. form of the output is:
    ///
    /// maple:
    ///                                                             Req'd  Req'd   Elap
    /// Job ID          Username Queue    Jobname    SessID NDS TSK Memory Time  S Time
    /// --------------- -------- -------- ---------- ------ --- --- ------ ----- - -----
    /// 819446          user     queue    C6HNpts      5085   1   1    8gb 26784 R 00:00
    fn stat_cmd(&self) -> String {
        let user = std::env::vars()
            .find(|x| x.0 == "USER")
            .expect("couldn't find $USER env var");
        let status = match Command::new("qstat").args(["-u", &user.1]).output()
        {
            Ok(status) => status,
            Err(e) => panic!("failed to run squeue with {e}"),
        };
        String::from_utf8(status.stdout)
            .expect("failed to convert squeue output to String")
    }

    fn status(&self) -> HashSet<String> {
        let mut ret = HashSet::new();
        let lines = <Pbs as SubQueue<P>>::stat_cmd(self);
        // skip to end of header
        let lines = lines.lines().skip_while(|l| !l.contains("-----------"));
        for line in lines {
            let fields: Vec<_> = line.split_whitespace().collect();
            assert!(fields.len() == 11);
            ret.insert(fields[0].to_string());
        }
        ret
    }

    fn no_del(&self) -> bool {
        self.no_del
    }
}
