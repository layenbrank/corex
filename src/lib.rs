// mod command;
// mod copy;
// mod legacy_notification;
// mod notification_example;
// mod process;
// mod util;

// pub use command::*;
// pub use legacy_notification::*;
// pub use notification_example::*;
// pub use process::*;

pub mod copy {
    pub mod controller;
    pub mod service;
}

pub mod generate {
    pub mod controller;
    pub mod service;
}

pub mod utils {
    pub mod verifier;
}
