/// Convenience macro for implementing the `Message` trait for a struct.
#[macro_export]
macro_rules! message {
    ($message: ty, $result: ty, $($bounds:tt)+) => {
        impl<$($bounds)+> coerce::actor::message::Message for $message {
            type Result = $result;
        }
    };
    ($message: ty, $result: ty) => {
        impl coerce::actor::message::Message for $message {
            type Result = $result;
        }
    };
}
