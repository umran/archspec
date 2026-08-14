pub mod request;
pub mod subscription;

pub use request::*;
pub use subscription::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Request(RequestInput),
    Subscription(SubscriptionInput),
}
