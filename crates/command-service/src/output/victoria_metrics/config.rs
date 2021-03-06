use reqwest::Url;
use structopt::StructOpt;

#[derive(Clone, Debug, StructOpt)]
pub struct VictoriaMetricsConfig {
    #[structopt(
        name = "victoria-metrics-url",
        long = "victoria-metrics-output-url",
        env = "VICTORIA_METRICS_OUTPUT_URL"
    )]
    pub url: Url,
}
