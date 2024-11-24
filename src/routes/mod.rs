mod health_check;
mod subscriptions;
mod subscriptions_confirm;
mod newsletters;

pub use health_check::*;
pub use subscriptions::*;
pub use subscriptions_confirm::*;
pub use newsletters::*;

pub fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut current = e.source();
    while let Some(e) = current {
        writeln!(f, "Caused by:\n\t{}", e)?;
        current = e.source()
    }
    Ok(())
}
