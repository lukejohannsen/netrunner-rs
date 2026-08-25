mod common;
mod corp;
mod identities;
mod registry;
mod runner;

#[cfg(test)]
mod tests;

pub use corp::register_corp_cards;
pub use identities::register_identities;
pub use registry::CardRegistry;
pub use runner::register_runner_cards;

/// Registers the full baseline Core Set suite — identities, Corp cards, and
/// Runner cards — into `registry`. The single entry point a caller (server,
/// gym, client) reaches for to get a populated `CardRegistry`; see each of
/// `register_identities`/`register_corp_cards`/`register_runner_cards` for
/// what it contributes.
pub fn register_baseline_set(registry: &mut CardRegistry) {
    register_identities(registry);
    register_corp_cards(registry);
    register_runner_cards(registry);
}
