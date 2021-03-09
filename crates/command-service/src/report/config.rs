use structopt::StructOpt;

#[derive(Clone, Debug, StructOpt)]
pub struct ReportServiceConfig {
    #[structopt(
        name = "report-destination",
        long = "report-destination",
        env = "REPORT_DESTINATION"
    )]
    pub destination: Option<String>,
}
