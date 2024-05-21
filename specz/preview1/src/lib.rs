pub mod witx;

#[cfg(test)]
mod tests {
    use std::io;

    use eyre::Context as _;
    use tracing::level_filters::LevelFilter;
    use tracing_error::ErrorLayer;
    use tracing_subscriber::{layer::SubscriberExt as _, EnvFilter};

    use super::*;

    #[test]
    fn ok() -> Result<(), eyre::Error> {
        color_eyre::install()?;
        tracing::subscriber::set_global_default(
            tracing_subscriber::Registry::default()
                .with(
                    EnvFilter::builder()
                        .with_env_var("WAZZI_LOG_LEVEL")
                        .with_default_directive(LevelFilter::INFO.into())
                        .from_env_lossy(),
                )
                .with(ErrorLayer::default())
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_thread_names(true)
                        .with_writer(io::stderr)
                        .pretty(),
                ),
        )
        .wrap_err("failed to configure tracing")?;

        let mut spec = wazzi_specz_wasi::Spec::new();

        witx::preview1(&mut spec).unwrap();

        Ok(())
    }
}



